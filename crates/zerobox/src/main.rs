mod debug;
mod profile;
mod snapshot;

use debug::debug_log;

use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use zerobox::Sandbox;

#[derive(Parser, Debug)]
#[command(name = "zerobox", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Option<CliSubcommand>,

    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub allow_read: Option<Vec<PathBuf>>,

    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub deny_read: Option<Vec<PathBuf>>,

    #[arg(long, value_delimiter = ',', num_args = 0..)]
    pub allow_write: Option<Vec<PathBuf>>,

    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub deny_write: Option<Vec<PathBuf>>,

    #[arg(long, value_delimiter = ',', num_args = 0..)]
    pub allow_net: Option<Vec<String>>,

    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub deny_net: Option<Vec<String>>,

    #[arg(long, short = 'A')]
    pub allow_all: bool,

    #[arg(long, short = 'C')]
    pub cwd: Option<PathBuf>,

    #[arg(long)]
    pub no_sandbox: bool,

    #[arg(long)]
    pub strict_sandbox: bool,

    #[arg(long = "env", value_name = "KEY=VALUE")]
    pub set_env: Vec<String>,

    #[arg(long, value_delimiter = ',', num_args = 0..)]
    pub allow_env: Option<Vec<String>>,

    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub deny_env: Option<Vec<String>>,

    #[arg(long = "secret", value_name = "KEY=VALUE")]
    pub secret: Vec<String>,

    #[arg(long = "secret-host", value_name = "KEY=HOSTS")]
    pub secret_host: Vec<String>,

    #[arg(long)]
    pub debug: bool,

    #[arg(long)]
    pub profile: Option<String>,

    #[arg(long)]
    pub snapshot: bool,

    #[arg(long)]
    pub restore: bool,

    #[arg(long = "snapshot-path", value_delimiter = ',', num_args = 1..)]
    pub snapshot_paths: Option<Vec<std::path::PathBuf>>,

    #[arg(long = "snapshot-exclude", value_delimiter = ',', num_args = 1..)]
    pub snapshot_exclude: Option<Vec<String>>,

    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum CliSubcommand {
    Snapshot {
        #[command(subcommand)]
        action: SnapshotAction,
    },
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProfileAction {
    List,
    Schema,
    Show { name: String },
}

#[derive(Subcommand, Debug)]
pub enum SnapshotAction {
    List,
    Diff {
        id: String,
    },
    Restore {
        id: String,
    },
    Clean {
        #[arg(long, default_value = "30")]
        older_than: u64,
    },
}

fn exit_code_from_status(status: std::process::ExitStatus) -> ExitCode {
    if let Some(code) = status.code() {
        return ExitCode::from(code as u8);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return ExitCode::from((128 + signal) as u8);
        }
    }
    ExitCode::from(1)
}

fn main() -> ExitCode {
    tokio_main()
}

#[tokio::main]
async fn tokio_main() -> ExitCode {
    #[cfg(target_os = "linux")]
    {
        use zerobox_sandboxing::landlock::ZEROBOX_LINUX_SANDBOX_ARG0;
        let exe_name = std::env::args_os()
            .next()
            .as_ref()
            .and_then(|s| Path::new(s).file_name().map(|f| f.to_os_string()));
        if exe_name.as_deref() == Some(std::ffi::OsStr::new(ZEROBOX_LINUX_SANDBOX_ARG0)) {
            zerobox_linux_sandbox::run_main();
        }
    }

    let cli = Cli::parse();

    if let Some(CliSubcommand::Snapshot { action }) = &cli.subcommand {
        return snapshot::handle_subcommand(action);
    }
    if let Some(CliSubcommand::Profile { action }) = &cli.subcommand {
        return profile::handle_subcommand(action);
    }

    if cli.command.is_empty() {
        eprintln!("error: no command specified");
        return ExitCode::from(1);
    }

    if cli.strict_sandbox && (cli.no_sandbox || cli.allow_all) {
        eprintln!("error: --strict-sandbox cannot be combined with --no-sandbox or --allow-all");
        return ExitCode::from(1);
    }

    let dbg = cli.debug;
    if dbg {
        debug::init_tracing();
    }

    let mut sandbox = Sandbox::command(&cli.command[0]).args(&cli.command[1..]);

    if let Some(ref cwd) = cli.cwd {
        sandbox = sandbox.cwd(cwd);
    }

    if cli.no_sandbox {
        sandbox = sandbox.no_sandbox().no_profile();
    } else if cli.allow_all {
        sandbox = sandbox.full_access().no_profile();
    } else {
        if cli.strict_sandbox {
            sandbox = sandbox.strict();
        }
        if let Some(ref name) = cli.profile {
            sandbox = sandbox.profile(name);
        }
    }
    // else: default profile loads automatically

    if let Some(ref paths) = cli.allow_read {
        for p in paths {
            sandbox = sandbox.allow_read(p);
        }
    }
    if let Some(ref paths) = cli.deny_read {
        for p in paths {
            sandbox = sandbox.deny_read(p);
        }
    }
    if let Some(ref paths) = cli.allow_write {
        if paths.is_empty() {
            sandbox = sandbox.allow_write_all();
        } else {
            for p in paths {
                sandbox = sandbox.allow_write(p);
            }
        }
    }
    if let Some(ref paths) = cli.deny_write {
        for p in paths {
            sandbox = sandbox.deny_write(p);
        }
    }

    if let Some(ref domains) = cli.allow_net {
        if domains.is_empty() {
            sandbox = sandbox.allow_net_all();
        } else {
            sandbox = sandbox.allow_net(domains);
        }
    }
    if let Some(ref domains) = cli.deny_net {
        sandbox = sandbox.deny_net(domains);
    }

    for pair in &cli.set_env {
        if let Some((key, value)) = pair.split_once('=') {
            sandbox = sandbox.env(key, value);
        } else {
            eprintln!("error: invalid --env value '{pair}': expected KEY=VALUE format");
            return ExitCode::from(1);
        }
    }
    if let Some(ref keys) = cli.allow_env {
        if keys.is_empty() {
            sandbox = sandbox.inherit_env();
        } else {
            sandbox = sandbox.allow_env(keys);
        }
    }
    if let Some(ref keys) = cli.deny_env {
        sandbox = sandbox.deny_env(keys);
    }

    for pair in &cli.secret {
        if let Some((key, value)) = pair.split_once('=') {
            if key.is_empty() {
                eprintln!("error: invalid --secret value '{pair}': key cannot be empty");
                return ExitCode::from(1);
            }
            sandbox = sandbox.secret(key, value);
        } else {
            eprintln!("error: invalid --secret value '{pair}': expected KEY=VALUE format");
            return ExitCode::from(1);
        }
    }
    for pair in &cli.secret_host {
        if let Some((key, hosts)) = pair.split_once('=') {
            sandbox = sandbox.secret_host(key, hosts);
        } else {
            eprintln!("error: invalid --secret-host value '{pair}': expected KEY=HOSTS format");
            return ExitCode::from(1);
        }
    }

    debug_log!(
        dbg,
        "cwd: {:?}",
        cli.cwd.as_deref().unwrap_or(Path::new("."))
    );

    let cwd = cli
        .cwd
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let do_snapshot = cli.snapshot || cli.restore;
    let snapshot_state = if do_snapshot {
        match snapshot::build_snapshot_state(&cli, &cwd) {
            Ok(mut state) => match state.manager.create_baseline() {
                Ok(baseline) => {
                    debug_log!(
                        dbg,
                        "snapshot: baseline captured ({} files)",
                        baseline.files.len()
                    );
                    Some((state, baseline))
                }
                Err(e) => {
                    if cli.restore {
                        eprintln!("error: --restore requires snapshot but baseline failed: {e:#}");
                        return ExitCode::from(1);
                    }
                    eprintln!("warning: snapshot baseline failed: {e:#}");
                    None
                }
            },
            Err(e) => {
                if cli.restore {
                    eprintln!("error: --restore requires snapshot but setup failed: {e:#}");
                    return ExitCode::from(1);
                }
                eprintln!("warning: snapshot setup failed: {e:#}");
                None
            }
        }
    } else {
        None
    };

    let is_tty = std::io::IsTerminal::is_terminal(&std::io::stdin());

    let (exit, raw_exit_code) = if is_tty {
        match sandbox.status().await {
            Ok(status) => (exit_code_from_status(status), status.code()),
            Err(e) => {
                eprintln!("error: {e:#}");
                (ExitCode::from(1), Some(1))
            }
        }
    } else {
        match sandbox.run().await {
            Ok(output) => {
                use std::io::Write;
                let _ = std::io::stdout().write_all(&output.stdout);
                let _ = std::io::stderr().write_all(&output.stderr);
                (exit_code_from_status(output.status), output.status.code())
            }
            Err(e) => {
                eprintln!("error: {e:#}");
                (ExitCode::from(1), Some(1))
            }
        }
    };

    if let Some((mut state, baseline)) = snapshot_state {
        let incremental = state.manager.create_incremental(&baseline);

        if let Ok((_, ref changes)) = incremental {
            snapshot::print_summary_to(changes, &mut std::io::stderr());
        } else if let Err(ref e) = incremental {
            eprintln!("snapshot: incremental failed: {e:#}");
        }

        let mut meta = snapshot::build_session_metadata(&state, &cli, &baseline);
        meta.ended = Some(chrono::Utc::now().to_rfc3339());
        meta.exit_code = raw_exit_code;
        meta.snapshot_count = state.manager.snapshot_count();
        if let Ok((ref final_manifest, _)) = incremental {
            meta.merkle_roots.push(final_manifest.merkle_root);
        }
        if let Err(e) = state.manager.save_session(&meta) {
            eprintln!("snapshot: failed to save session: {e:#}");
        }

        if cli.restore {
            match state.manager.restore_to(&baseline) {
                Ok(applied) if !applied.is_empty() => {
                    eprintln!("snapshot: restored {} files", applied.len());
                }
                Err(e) => {
                    eprintln!("snapshot: restore failed: {e:#}");
                    return ExitCode::from(1);
                }
                _ => {}
            }
        }
    }

    exit
}
