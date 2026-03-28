use anyhow::{anyhow, Result};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

/// Extract a human-readable error message from an API JSON response.
fn api_error(status: u16, resp: &serde_json::Value) -> anyhow::Error {
    if let Some(msg) = resp["error"].as_str() {
        anyhow!("{}", msg)
    } else {
        anyhow!("HTTP {} — {}", status, resp)
    }
}

/// Simple HTTP client for the zerobox daemon API.
pub struct DaemonClient {
    base_url: String,
    client: Client<hyper_util::client::legacy::connect::HttpConnector, Full<Bytes>>,
}

impl DaemonClient {
    pub fn new(endpoint: &str) -> Self {
        let client = Client::builder(TokioExecutor::new()).build_http();
        Self {
            base_url: format!("{}/v1", endpoint.trim_end_matches('/')),
            client,
        }
    }

    async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<(u16, serde_json::Value)> {
        let uri: hyper::Uri = format!("{}{}", self.base_url, path).parse()?;
        let method = hyper::Method::from_bytes(method.as_bytes())?;

        let body_bytes = match body {
            Some(val) => serde_json::to_vec(&val)?,
            None => Vec::new(),
        };

        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("Content-Type", "application/json")
            .body(Full::new(Bytes::from(body_bytes)))?;

        let resp = self.client.request(req).await.map_err(|e| {
            anyhow!(
                "Failed to connect to daemon. Is it running?\n  Error: {}",
                e
            )
        })?;

        let status = resp.status().as_u16();
        let body = resp.into_body().collect().await?.to_bytes();

        if body.is_empty() {
            return Ok((status, serde_json::json!(null)));
        }

        let json: serde_json::Value = serde_json::from_slice(&body)
            .unwrap_or(serde_json::json!({ "raw": String::from_utf8_lossy(&body).to_string() }));

        Ok((status, json))
    }

    pub async fn create_sandbox(
        &self,
        image: Option<&str>,
        vcpus: Option<u32>,
        memory: Option<u32>,
        timeout: Option<u64>,
        ports: &[u16],
    ) -> Result<serde_json::Value> {
        let mut body = serde_json::json!({});

        if image.is_some() || vcpus.is_some() || memory.is_some() {
            let mut source = serde_json::Map::new();
            if let Some(img) = image {
                source.insert("type".into(), "image".into());
                source.insert("image".into(), img.into());
            }
            if !source.is_empty() {
                body["source"] = serde_json::Value::Object(source);
            }
        }

        if vcpus.is_some() || memory.is_some() {
            let mut res = serde_json::Map::new();
            if let Some(v) = vcpus {
                res.insert("vcpus".into(), v.into());
            }
            if let Some(m) = memory {
                res.insert("memoryMib".into(), m.into());
            }
            body["resources"] = serde_json::Value::Object(res);
        }

        if let Some(t) = timeout {
            body["timeout"] = t.into();
        }

        if !ports.is_empty() {
            body["ports"] = ports.iter().map(|&p| serde_json::Value::from(p)).collect();
        }

        let (status, resp) = self.request("POST", "/sandboxes", Some(body)).await?;
        if status >= 400 {
            return Err(api_error(status, &resp));
        }
        Ok(resp)
    }

    pub async fn list_sandboxes(&self) -> Result<serde_json::Value> {
        let (_, resp) = self.request("GET", "/sandboxes", None).await?;
        Ok(resp)
    }

    pub async fn get_sandbox(&self, id: &str) -> Result<serde_json::Value> {
        let (status, resp) = self
            .request("GET", &format!("/sandboxes/{}", id), None)
            .await?;
        if status == 404 {
            return Err(anyhow!("Sandbox not found: {}", id));
        }
        Ok(resp)
    }

    pub async fn stop_sandbox(&self, id: &str) -> Result<serde_json::Value> {
        let (status, resp) = self
            .request("POST", &format!("/sandboxes/{}/stop", id), None)
            .await?;
        if status >= 400 {
            return Err(api_error(status, &resp));
        }
        Ok(resp)
    }

    pub async fn destroy_sandbox(&self, id: &str) -> Result<()> {
        let (status, resp) = self
            .request("DELETE", &format!("/sandboxes/{}", id), None)
            .await?;
        if status >= 400 {
            return Err(api_error(status, &resp));
        }
        Ok(())
    }

    pub async fn exec_command(
        &self,
        sandbox_id: &str,
        cmd: &str,
        args: &[String],
    ) -> Result<serde_json::Value> {
        let body = serde_json::json!({
            "cmd": cmd,
            "args": args,
        });
        let (status, resp) = self
            .request(
                "POST",
                &format!("/sandboxes/{}/commands", sandbox_id),
                Some(body),
            )
            .await?;
        if status >= 400 {
            return Err(api_error(status, &resp));
        }
        Ok(resp)
    }

    pub async fn create_snapshot(&self, sandbox_id: &str) -> Result<serde_json::Value> {
        let (status, resp) = self
            .request("POST", &format!("/sandboxes/{}/snapshot", sandbox_id), None)
            .await?;
        if status >= 400 {
            return Err(api_error(status, &resp));
        }
        Ok(resp)
    }

    pub async fn list_snapshots(&self) -> Result<serde_json::Value> {
        let (_, resp) = self.request("GET", "/snapshots", None).await?;
        Ok(resp)
    }

    pub async fn delete_snapshot(&self, id: &str) -> Result<()> {
        let (status, resp) = self
            .request("DELETE", &format!("/snapshots/{}", id), None)
            .await?;
        if status >= 400 {
            return Err(api_error(status, &resp));
        }
        Ok(())
    }
}

/// Connect to a sandbox shell via the vsock UDS.
///
/// Opens a direct connection to the guest agent, sends the `shell` RPC,
/// then proxies raw bytes between the terminal and the PTY inside the VM.
pub async fn connect_shell(vsock_path: &str) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;
    use zerobox_common::protocol::*;

    let stream = UnixStream::connect(vsock_path)
        .await
        .map_err(|e| anyhow!("Failed to connect to VM: {}", e))?;

    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Firecracker vsock handshake: CONNECT <port>
    writer
        .write_all(format!("CONNECT {}\n", VSOCK_PORT).as_bytes())
        .await?;
    let mut ok_line = String::new();
    reader.read_line(&mut ok_line).await?;
    if !ok_line.starts_with("OK") {
        return Err(anyhow!("Vsock handshake failed: {}", ok_line.trim()));
    }

    // Send shell RPC
    let req = RpcRequest {
        id: 1,
        method: METHOD_SHELL.to_string(),
        params: serde_json::json!({}),
    };
    let mut req_json = serde_json::to_string(&req)?;
    req_json.push('\n');
    writer.write_all(req_json.as_bytes()).await?;

    // Read JSON response
    let mut resp_line = String::new();
    reader.read_line(&mut resp_line).await?;
    let resp: RpcResponse =
        serde_json::from_str(&resp_line).map_err(|e| anyhow!("Invalid shell response: {}", e))?;
    if let Some(err) = resp.error {
        return Err(anyhow!("Shell failed: {}", err.message));
    }

    // Connection is now in raw byte mode. Set terminal to raw mode.
    let _raw_guard = RawTerminal::enter()?;

    eprintln!("\r\nConnected. Press Ctrl-D to disconnect.\r");

    // Get the inner reader (BufReader may have buffered data)
    let mut inner_reader = reader.into_inner();

    // Bidirectional proxy: terminal stdin <-> vsock
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    let mut stdin_buf = [0u8; 1024];
    let mut vsock_buf = [0u8; 4096];

    loop {
        tokio::select! {
            // Terminal -> VM
            n = stdin.read(&mut stdin_buf) => {
                match n {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if writer.write_all(&stdin_buf[..n]).await.is_err() {
                            break;
                        }
                    }
                }
            }
            // VM -> Terminal
            n = inner_reader.read(&mut vsock_buf) => {
                match n {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if stdout.write_all(&vsock_buf[..n]).await.is_err() {
                            break;
                        }
                        stdout.flush().await.ok();
                    }
                }
            }
        }
    }

    drop(_raw_guard); // restore terminal
    eprintln!("\r\nDisconnected.");
    Ok(())
}

/// RAII guard that puts the terminal in raw mode and restores it on drop.
struct RawTerminal {
    original: libc::termios,
}

impl RawTerminal {
    fn enter() -> Result<Self> {
        unsafe {
            let mut original: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut original) != 0 {
                return Err(anyhow!("tcgetattr failed"));
            }

            let mut raw = original;
            libc::cfmakeraw(&mut raw);
            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) != 0 {
                return Err(anyhow!("tcsetattr failed"));
            }

            Ok(Self { original })
        }
    }
}

impl Drop for RawTerminal {
    fn drop(&mut self) {
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.original);
        }
    }
}
