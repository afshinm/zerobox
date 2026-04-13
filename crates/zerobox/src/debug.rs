pub fn log(msg: &str) {
    use std::io::IsTerminal;
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

pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(false)
        .with_writer(std::io::stderr)
        .init();
}
