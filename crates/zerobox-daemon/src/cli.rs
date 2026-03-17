use anyhow::{anyhow, Result};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

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
            return Err(anyhow!("Error ({}): {}", status, resp));
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
            return Err(anyhow!("Error ({}): {}", status, resp));
        }
        Ok(resp)
    }

    pub async fn destroy_sandbox(&self, id: &str) -> Result<()> {
        let (status, resp) = self
            .request("DELETE", &format!("/sandboxes/{}", id), None)
            .await?;
        if status >= 400 {
            return Err(anyhow!("Error ({}): {}", status, resp));
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
            return Err(anyhow!("Error ({}): {}", status, resp));
        }
        Ok(resp)
    }

    pub async fn create_snapshot(&self, sandbox_id: &str) -> Result<serde_json::Value> {
        let (status, resp) = self
            .request("POST", &format!("/sandboxes/{}/snapshot", sandbox_id), None)
            .await?;
        if status >= 400 {
            return Err(anyhow!("Error ({}): {}", status, resp));
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
            return Err(anyhow!("Error ({}): {}", status, resp));
        }
        Ok(())
    }
}
