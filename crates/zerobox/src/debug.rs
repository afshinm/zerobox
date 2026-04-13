use std::io::IsTerminal;
use std::sync::Arc;

use async_trait::async_trait;
use zerobox_network_proxy::{BlockedRequest, BlockedRequestObserver};

pub fn log(msg: &str) {
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ");
    if std::io::stderr().is_terminal() {
        eprintln!("\x1b[2m{ts}\x1b[0m {msg}");
    } else {
        eprintln!("{ts} {msg}");
    }
}

macro_rules! debug_log {
    ($debug:expr, $($arg:tt)*) => {
        if $debug {
            $crate::debug::log(&format!($($arg)*));
        }
    };
}
pub(crate) use debug_log;

pub struct DebugObserver;

#[async_trait]
impl BlockedRequestObserver for DebugObserver {
    async fn on_blocked_request(&self, req: BlockedRequest) {
        let port = req.port.map(|p| format!(":{p}")).unwrap_or_default();
        log(&format!("blocked: {}{port} ({})", req.host, req.reason));
    }
}

pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(false)
        .with_writer(std::io::stderr)
        .init();
}

pub fn blocked_observer() -> Arc<dyn BlockedRequestObserver> {
    Arc::new(DebugObserver)
}
