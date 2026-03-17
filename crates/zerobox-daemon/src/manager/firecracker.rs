use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;

/// Client for the Firecracker REST API, communicating over a Unix domain socket.
pub struct FirecrackerClient {
    socket_path: PathBuf,
}

impl FirecrackerClient {
    pub fn new(socket_path: impl AsRef<Path>) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
        }
    }

    /// Sends an HTTP request to the Firecracker API socket and returns the response body.
    async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<Vec<u8>> {
        let stream = UnixStream::connect(&self.socket_path).await.map_err(|e| {
            anyhow!(
                "Failed to connect to Firecracker socket at {:?}: {}",
                self.socket_path,
                e
            )
        })?;

        let io = TokioIo::new(stream);

        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;

        // Spawn connection driver
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                tracing::error!("Firecracker HTTP connection error: {}", e);
            }
        });

        let method = hyper::Method::from_bytes(method.as_bytes())
            .map_err(|e| anyhow!("Invalid HTTP method '{}': {}", method, e))?;

        let body_bytes = match body {
            Some(val) => serde_json::to_vec(&val)?,
            None => Vec::new(),
        };

        let req = Request::builder()
            .method(method)
            .uri(format!("http://localhost{}", path))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(Full::new(Bytes::from(body_bytes)))?;

        let response = sender.send_request(req).await?;
        let status = response.status();
        let body = response.into_body().collect().await?.to_bytes().to_vec();

        if !status.is_success() {
            let body_str = String::from_utf8_lossy(&body);
            return Err(anyhow!("Firecracker API error ({}): {}", status, body_str));
        }

        Ok(body)
    }

    /// PUT /boot-source
    pub async fn set_boot_source(&self, kernel_image_path: &str, boot_args: &str) -> Result<()> {
        let body = serde_json::json!({
            "kernel_image_path": kernel_image_path,
            "boot_args": boot_args,
        });
        self.request("PUT", "/boot-source", Some(body)).await?;
        Ok(())
    }

    /// PUT /drives/{drive_id}
    pub async fn set_drives(
        &self,
        drive_id: &str,
        path: &str,
        is_root: bool,
        is_read_only: bool,
    ) -> Result<()> {
        let body = serde_json::json!({
            "drive_id": drive_id,
            "path_on_host": path,
            "is_root_device": is_root,
            "is_read_only": is_read_only,
        });
        self.request("PUT", &format!("/drives/{}", drive_id), Some(body))
            .await?;
        Ok(())
    }

    /// PUT /machine-config
    pub async fn set_machine_config(&self, vcpus: u32, mem_mib: u32) -> Result<()> {
        let body = serde_json::json!({
            "vcpu_count": vcpus,
            "mem_size_mib": mem_mib,
        });
        self.request("PUT", "/machine-config", Some(body)).await?;
        Ok(())
    }

    /// PUT /vsock
    pub async fn set_vsock(&self, vsock_id: &str, guest_cid: u32, uds_path: &str) -> Result<()> {
        let body = serde_json::json!({
            "vsock_id": vsock_id,
            "guest_cid": guest_cid,
            "uds_path": uds_path,
        });
        self.request("PUT", "/vsock", Some(body)).await?;
        Ok(())
    }

    /// PUT /network-interfaces/{iface_id}
    pub async fn set_network_interface(&self, iface_id: &str, host_dev_name: &str) -> Result<()> {
        let body = serde_json::json!({
            "iface_id": iface_id,
            "host_dev_name": host_dev_name,
        });
        self.request(
            "PUT",
            &format!("/network-interfaces/{}", iface_id),
            Some(body),
        )
        .await?;
        Ok(())
    }

    /// PUT /actions — InstanceStart
    pub async fn start_instance(&self) -> Result<()> {
        let body = serde_json::json!({
            "action_type": "InstanceStart",
        });
        self.request("PUT", "/actions", Some(body)).await?;
        Ok(())
    }

    /// PUT /actions — SendCtrlAltDel (graceful stop)
    pub async fn stop_instance(&self) -> Result<()> {
        let body = serde_json::json!({
            "action_type": "SendCtrlAltDel",
        });
        self.request("PUT", "/actions", Some(body)).await?;
        Ok(())
    }

    /// PATCH /vm — Pause
    pub async fn pause_instance(&self) -> Result<()> {
        let body = serde_json::json!({
            "state": "Paused",
        });
        self.request("PATCH", "/vm", Some(body)).await?;
        Ok(())
    }

    /// PATCH /vm — Resume
    pub async fn resume_instance(&self) -> Result<()> {
        let body = serde_json::json!({
            "state": "Resumed",
        });
        self.request("PATCH", "/vm", Some(body)).await?;
        Ok(())
    }

    /// PUT /snapshot/create
    pub async fn create_snapshot(&self, snapshot_path: &str, mem_path: &str) -> Result<()> {
        let body = serde_json::json!({
            "snapshot_type": "Full",
            "snapshot_path": snapshot_path,
            "mem_file_path": mem_path,
        });
        self.request("PUT", "/snapshot/create", Some(body)).await?;
        Ok(())
    }

    /// PUT /snapshot/load
    pub async fn load_snapshot(&self, snapshot_path: &str, mem_path: &str) -> Result<()> {
        let body = serde_json::json!({
            "snapshot_path": snapshot_path,
            "mem_backend": {
                "backend_type": "File",
                "backend_path": mem_path,
            },
        });
        self.request("PUT", "/snapshot/load", Some(body)).await?;
        Ok(())
    }
}
