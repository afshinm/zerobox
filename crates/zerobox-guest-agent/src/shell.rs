use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use anyhow::{Context, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::info;

/// Spawns an interactive bash shell with a PTY and streams I/O.
///
/// After the caller sends the JSON-RPC `shell` request and receives the
/// response, the connection switches to raw byte mode.
pub async fn handle_shell<R, W>(mut reader: R, mut writer: W) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    // Open PTY pair via libc
    let mut master_fd: libc::c_int = 0;
    let mut slave_fd: libc::c_int = 0;
    let rc = unsafe {
        libc::openpty(
            &mut master_fd,
            &mut slave_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        return Err(anyhow::anyhow!(
            "openpty failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    info!("Spawning interactive shell");

    // Spawn bash attached to the slave PTY
    let child = unsafe {
        std::process::Command::new("/bin/bash")
            .arg("--login")
            .arg("-i")
            .stdin(std::process::Stdio::from_raw_fd(slave_fd))
            .stdout(std::process::Stdio::from_raw_fd(slave_fd))
            .stderr(std::process::Stdio::from_raw_fd(slave_fd))
            .spawn()
            .context("failed to spawn bash")?
    };

    // Close slave in parent — child owns it
    unsafe {
        libc::close(slave_fd);
    }

    info!(pid = child.id(), "Shell started");

    // Wrap master fd in async I/O
    let master_owned = unsafe { OwnedFd::from_raw_fd(master_fd) };
    let master_async = tokio::io::unix::AsyncFd::new(master_owned)?;

    let mut client_buf = [0u8; 4096];
    let mut pty_buf = [0u8; 4096];

    loop {
        tokio::select! {
            // Client -> PTY (user input)
            result = reader.read(&mut client_buf) => {
                match result {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut guard = master_async.writable().await?;
                        match guard.try_io(|fd| {
                            let written = unsafe { libc::write(fd.as_raw_fd(), client_buf.as_ptr() as *const libc::c_void, n) };
                            if written < 0 { Err(std::io::Error::last_os_error()) } else { Ok(written as usize) }
                        }) {
                            Ok(Ok(_)) => {}
                            _ => break,
                        }
                    }
                }
            }

            // PTY -> Client (shell output)
            result = master_async.readable() => {
                match result {
                    Err(_) => break,
                    Ok(mut guard) => {
                        match guard.try_io(|fd| {
                            let n = unsafe { libc::read(fd.as_raw_fd(), pty_buf.as_mut_ptr() as *mut libc::c_void, pty_buf.len()) };
                            if n < 0 { Err(std::io::Error::last_os_error()) } else { Ok(n as usize) }
                        }) {
                            Ok(Ok(0)) => break,
                            Ok(Ok(n)) => {
                                if writer.write_all(&pty_buf[..n]).await.is_err() { break; }
                                writer.flush().await.ok();
                            }
                            Ok(Err(_)) => break,
                            Err(_) => continue, // WouldBlock
                        }
                    }
                }
            }
        }
    }

    info!("Shell session ended");
    Ok(())
}
