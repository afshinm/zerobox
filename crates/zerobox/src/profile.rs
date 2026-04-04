//! Profile system: named presets of CLI flags with inheritance.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{Cli, ProfileAction};

const MAX_INHERITANCE_DEPTH: usize = 100;

const BUILTIN_PROFILES: &[(&str, &str)] = &[
    // Deny configs (leaf building blocks)
    (
        "deny-credentials",
        include_str!("../profiles/deny-credentials.json"),
    ),
    (
        "deny-shell-history",
        include_str!("../profiles/deny-shell-history.json"),
    ),
    (
        "deny-shell-configs",
        include_str!("../profiles/deny-shell-configs.json"),
    ),
    (
        "deny-keychains-macos",
        include_str!("../profiles/deny-keychains-macos.json"),
    ),
    (
        "deny-keychains-linux",
        include_str!("../profiles/deny-keychains-linux.json"),
    ),
    (
        "deny-browser-data-macos",
        include_str!("../profiles/deny-browser-data-macos.json"),
    ),
    (
        "deny-browser-data-linux",
        include_str!("../profiles/deny-browser-data-linux.json"),
    ),
    (
        "deny-macos-private",
        include_str!("../profiles/deny-macos-private.json"),
    ),
    // System configs
    (
        "system-read-macos",
        include_str!("../profiles/system-read-macos.json"),
    ),
    (
        "system-read-linux",
        include_str!("../profiles/system-read-linux.json"),
    ),
    (
        "system-write-macos",
        include_str!("../profiles/system-write-macos.json"),
    ),
    (
        "system-write-linux",
        include_str!("../profiles/system-write-linux.json"),
    ),
    (
        "linux-runtime-state",
        include_str!("../profiles/linux-runtime-state.json"),
    ),
    (
        "linux-sysfs-read",
        include_str!("../profiles/linux-sysfs-read.json"),
    ),
    (
        "linux-temp-read",
        include_str!("../profiles/linux-temp-read.json"),
    ),
    // Cache configs
    ("cache-macos", include_str!("../profiles/cache-macos.json")),
    ("cache-linux", include_str!("../profiles/cache-linux.json")),
    // Runtime configs
    (
        "node-runtime",
        include_str!("../profiles/node-runtime.json"),
    ),
    (
        "python-runtime",
        include_str!("../profiles/python-runtime.json"),
    ),
    (
        "rust-runtime",
        include_str!("../profiles/rust-runtime.json"),
    ),
    ("go-runtime", include_str!("../profiles/go-runtime.json")),
    (
        "homebrew-macos",
        include_str!("../profiles/homebrew-macos.json"),
    ),
    (
        "homebrew-linux",
        include_str!("../profiles/homebrew-linux.json"),
    ),
    (
        "nix-runtime",
        include_str!("../profiles/nix-runtime.json"),
    ),
    ("user-tools", include_str!("../profiles/user-tools.json")),
    // Tool and app-specific configs
    ("git-config", include_str!("../profiles/git-config.json")),
    (
        "vscode-macos",
        include_str!("../profiles/vscode-macos.json"),
    ),
    (
        "vscode-linux",
        include_str!("../profiles/vscode-linux.json"),
    ),
    (
        "claude-code-macos",
        include_str!("../profiles/claude-code-macos.json"),
    ),
    (
        "claude-code-linux",
        include_str!("../profiles/claude-code-linux.json"),
    ),
    (
        "codex-macos",
        include_str!("../profiles/codex-macos.json"),
    ),
    (
        "opencode-linux",
        include_str!("../profiles/opencode-linux.json"),
    ),
    // Compositions
    ("hardened", include_str!("../profiles/hardened.json")),
    ("ai-agent", include_str!("../profiles/ai-agent.json")),
    // App profiles
    ("claude-code", include_str!("../profiles/claude-code.json")),
    ("codex", include_str!("../profiles/codex.json")),
    ("opencode", include_str!("../profiles/opencode.json")),
    ("openclaw", include_str!("../profiles/openclaw.json")),
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, rename = "use", deserialize_with = "deserialize_use")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    pub allow_read: Option<Vec<String>>,
    pub deny_read: Option<Vec<String>>,
    pub allow_write: Option<Vec<String>>,
    pub deny_write: Option<Vec<String>>,

    pub allow_net: Option<Vec<String>>,
    pub deny_net: Option<Vec<String>>,

    pub set_env: Option<HashMap<String, String>>,
    pub allow_env: Option<Vec<String>>,
    pub deny_env: Option<Vec<String>>,

    pub secret_hosts: Option<HashMap<String, Vec<String>>>,

    pub allow_all: Option<bool>,
    pub no_sandbox: Option<bool>,
    pub strict_sandbox: Option<bool>,

    pub snapshot: Option<bool>,
    pub restore: Option<bool>,
    pub snapshot_paths: Option<Vec<String>>,
    pub snapshot_exclude: Option<Vec<String>>,

    pub debug: Option<bool>,
}

fn deserialize_use<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        Single(String),
        Multiple(Vec<String>),
    }
    match Option::<StringOrVec>::deserialize(d)? {
        Some(StringOrVec::Single(s)) => Ok(vec![s]),
        Some(StringOrVec::Multiple(v)) => Ok(v),
        None => Ok(Vec::new()),
    }
}

fn expand_templates(s: &str, home: &Path, cwd: &Path, tmpdir: &Path) -> String {
    s.replace("$HOME", &home.to_string_lossy())
        .replace("$CWD", &cwd.to_string_lossy())
        .replace("$TMPDIR", &tmpdir.to_string_lossy())
}

fn expand_vec(v: &mut Option<Vec<String>>, home: &Path, cwd: &Path, tmpdir: &Path) {
    if let Some(items) = v {
        for item in items.iter_mut() {
            *item = expand_templates(item, home, cwd, tmpdir);
        }
    }
}

fn expand_profile(p: &mut Profile, home: &Path, cwd: &Path, tmpdir: &Path) {
    expand_vec(&mut p.allow_read, home, cwd, tmpdir);
    expand_vec(&mut p.deny_read, home, cwd, tmpdir);
    expand_vec(&mut p.allow_write, home, cwd, tmpdir);
    expand_vec(&mut p.deny_write, home, cwd, tmpdir);
    expand_vec(&mut p.snapshot_paths, home, cwd, tmpdir);
    if let Some(ref mut env) = p.set_env {
        for val in env.values_mut() {
            *val = expand_templates(val, home, cwd, tmpdir);
        }
    }
}

fn dedup_append(base: &Option<Vec<String>>, child: &Option<Vec<String>>) -> Option<Vec<String>> {
    match (base, child) {
        (None, None) => None,
        (Some(b), None) => Some(b.clone()),
        (None, Some(c)) => Some(c.clone()),
        (Some(b), Some(c)) => {
            let mut result = b.clone();
            for item in c {
                if !result.contains(item) {
                    result.push(item.clone());
                }
            }
            Some(result)
        }
    }
}

fn merge_maps(
    base: &Option<HashMap<String, String>>,
    child: &Option<HashMap<String, String>>,
) -> Option<HashMap<String, String>> {
    match (base, child) {
        (None, None) => None,
        (Some(b), None) => Some(b.clone()),
        (None, Some(c)) => Some(c.clone()),
        (Some(b), Some(c)) => {
            let mut result = b.clone();
            result.extend(c.iter().map(|(k, v)| (k.clone(), v.clone())));
            Some(result)
        }
    }
}

fn merge_host_maps(
    base: &Option<HashMap<String, Vec<String>>>,
    child: &Option<HashMap<String, Vec<String>>>,
) -> Option<HashMap<String, Vec<String>>> {
    match (base, child) {
        (None, None) => None,
        (Some(b), None) => Some(b.clone()),
        (None, Some(c)) => Some(c.clone()),
        (Some(b), Some(c)) => {
            let mut result = b.clone();
            for (k, cv) in c {
                let entry = result.entry(k.clone()).or_default();
                for v in cv {
                    if !entry.contains(v) {
                        entry.push(v.clone());
                    }
                }
            }
            Some(result)
        }
    }
}

fn merge_profiles(base: &Profile, child: &Profile) -> Profile {
    Profile {
        schema: None,
        description: child.description.clone().or(base.description.clone()),
        uses: Vec::new(),
        platform: None,
        allow_read: dedup_append(&base.allow_read, &child.allow_read),
        deny_read: dedup_append(&base.deny_read, &child.deny_read),
        allow_write: dedup_append(&base.allow_write, &child.allow_write),
        deny_write: dedup_append(&base.deny_write, &child.deny_write),
        allow_net: dedup_append(&base.allow_net, &child.allow_net),
        deny_net: dedup_append(&base.deny_net, &child.deny_net),
        set_env: merge_maps(&base.set_env, &child.set_env),
        allow_env: dedup_append(&base.allow_env, &child.allow_env),
        deny_env: dedup_append(&base.deny_env, &child.deny_env),
        secret_hosts: merge_host_maps(&base.secret_hosts, &child.secret_hosts),
        allow_all: child.allow_all.or(base.allow_all),
        no_sandbox: child.no_sandbox.or(base.no_sandbox),
        strict_sandbox: child.strict_sandbox.or(base.strict_sandbox),
        snapshot: child.snapshot.or(base.snapshot),
        restore: child.restore.or(base.restore),
        snapshot_paths: dedup_append(&base.snapshot_paths, &child.snapshot_paths),
        snapshot_exclude: dedup_append(&base.snapshot_exclude, &child.snapshot_exclude),
        debug: child.debug.or(base.debug),
    }
}

fn load_raw(name: &str) -> Result<Profile> {
    // User profiles shadow built-ins.
    let user_path = crate::zerobox_home()
        .join("profiles")
        .join(format!("{name}.json"));
    if user_path.exists() {
        let json = std::fs::read_to_string(&user_path)
            .with_context(|| format!("failed to read profile {}", user_path.display()))?;
        return serde_json::from_str(&json)
            .with_context(|| format!("invalid profile {}", user_path.display()));
    }

    for (builtin_name, json) in BUILTIN_PROFILES {
        if *builtin_name == name {
            return serde_json::from_str(json)
                .with_context(|| format!("invalid built-in profile '{name}'"));
        }
    }

    bail!("profile not found: '{name}'")
}

fn current_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    }
}

fn platform_matches(profile: &Profile) -> bool {
    match &profile.platform {
        Some(p) => p == current_platform(),
        None => true,
    }
}

fn resolve(name: &str, visited: &mut Vec<String>, depth: usize) -> Result<Profile> {
    if depth > MAX_INHERITANCE_DEPTH {
        bail!("profile composition too deep (max {MAX_INHERITANCE_DEPTH})");
    }
    if visited.contains(&name.to_string()) {
        bail!("circular profile reference: {}", visited.join(" -> "));
    }
    visited.push(name.to_string());

    let profile = load_raw(name)?;

    if !platform_matches(&profile) {
        return Ok(Profile::default());
    }

    if profile.uses.is_empty() {
        return Ok(profile);
    }

    let mut merged = Profile::default();
    for dep_name in &profile.uses {
        let dep = resolve(dep_name, visited, depth + 1)?;
        merged = merge_profiles(&merged, &dep);
    }

    Ok(merge_profiles(&merged, &profile))
}

pub fn load_and_apply(name: &str, cli: &mut Cli, cwd: &Path) -> Result<()> {
    let mut visited = Vec::new();
    let mut profile = resolve(name, &mut visited, 0)?;

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let tmpdir = std::env::temp_dir();
    expand_profile(&mut profile, &home, cwd, &tmpdir);

    apply_to_cli(cli, &profile);
    Ok(())
}

fn apply_to_cli(cli: &mut Cli, profile: &Profile) {
    fn apply_vec(cli_val: &mut Option<Vec<PathBuf>>, profile_val: &Option<Vec<String>>) {
        if cli_val.is_none()
            && let Some(pv) = profile_val
        {
            *cli_val = Some(pv.iter().map(PathBuf::from).collect());
        }
    }

    fn apply_str_vec(cli_val: &mut Option<Vec<String>>, profile_val: &Option<Vec<String>>) {
        if cli_val.is_none() {
            *cli_val = profile_val.clone();
        }
    }

    fn apply_bool(cli_val: &mut bool, profile_val: Option<bool>) {
        if !*cli_val && let Some(pv) = profile_val {
            *cli_val = pv;
        }
    }

    apply_vec(&mut cli.allow_read, &profile.allow_read);
    apply_vec(&mut cli.deny_read, &profile.deny_read);
    apply_vec(&mut cli.allow_write, &profile.allow_write);
    apply_vec(&mut cli.deny_write, &profile.deny_write);
    apply_str_vec(&mut cli.allow_net, &profile.allow_net);
    apply_str_vec(&mut cli.deny_net, &profile.deny_net);
    apply_str_vec(&mut cli.allow_env, &profile.allow_env);
    apply_str_vec(&mut cli.deny_env, &profile.deny_env);
    apply_vec(&mut cli.snapshot_paths, &profile.snapshot_paths);
    apply_str_vec(&mut cli.snapshot_exclude, &profile.snapshot_exclude);

    apply_bool(&mut cli.allow_all, profile.allow_all);
    apply_bool(&mut cli.no_sandbox, profile.no_sandbox);
    apply_bool(&mut cli.strict_sandbox, profile.strict_sandbox);
    apply_bool(&mut cli.snapshot, profile.snapshot);
    apply_bool(&mut cli.restore, profile.restore);
    apply_bool(&mut cli.debug, profile.debug);

    // set_env: profile provides base, CLI --env adds on top.
    if let Some(ref env_map) = profile.set_env {
        let mut profile_envs: Vec<String> =
            env_map.iter().map(|(k, v)| format!("{k}={v}")).collect();
        // CLI --env entries override profile keys.
        let cli_keys: Vec<String> = cli
            .set_env
            .iter()
            .filter_map(|e| e.split('=').next().map(String::from))
            .collect();
        profile_envs.retain(|e| {
            let key = e.split('=').next().unwrap_or("");
            !cli_keys.contains(&key.to_string())
        });
        profile_envs.append(&mut cli.set_env);
        cli.set_env = profile_envs;
    }

    // secret_hosts: merge profile hosts with CLI --secret-host.
    if let Some(ref hosts_map) = profile.secret_hosts {
        let mut profile_hosts: Vec<String> = hosts_map
            .iter()
            .map(|(k, hosts)| format!("{k}={}", hosts.join(",")))
            .collect();
        let cli_keys: Vec<String> = cli
            .secret_host
            .iter()
            .filter_map(|e| e.split('=').next().map(String::from))
            .collect();
        profile_hosts.retain(|e| {
            let key = e.split('=').next().unwrap_or("");
            !cli_keys.contains(&key.to_string())
        });
        profile_hosts.append(&mut cli.secret_host);
        cli.secret_host = profile_hosts;
    }
}

pub fn handle_subcommand(action: &ProfileAction) -> ExitCode {
    match action {
        ProfileAction::List => cmd_list(),
        ProfileAction::Show { name } => cmd_show(name),
    }
}

fn cmd_list() -> ExitCode {
    let use_color = std::io::IsTerminal::is_terminal(&std::io::stdout());
    let mut tw = tabwriter::TabWriter::new(std::io::stdout())
        .padding(2)
        .ansi(true);
    use std::io::Write;
    use zerobox_snapshot::dim;

    if use_color {
        let _ = writeln!(
            tw,
            "{}\t{}\t{}",
            dim("PROFILE"),
            dim("SOURCE"),
            dim("DESCRIPTION")
        );
    } else {
        let _ = writeln!(tw, "PROFILE\tSOURCE\tDESCRIPTION");
    }

    for (name, json) in BUILTIN_PROFILES {
        let desc = serde_json::from_str::<Profile>(json)
            .ok()
            .and_then(|p| p.description)
            .unwrap_or_default();
        let _ = writeln!(tw, "{name}\tbuilt-in\t{desc}");
    }

    let user_dir = crate::zerobox_home().join("profiles");
    if let Ok(entries) = std::fs::read_dir(&user_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let name = path.file_stem().unwrap_or_default().to_string_lossy();
                let desc = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|json| serde_json::from_str::<Profile>(&json).ok())
                    .and_then(|p| p.description)
                    .unwrap_or_default();
                let _ = writeln!(tw, "{name}\tuser\t{desc}");
            }
        }
    }

    let _ = tw.flush();
    ExitCode::SUCCESS
}

fn cmd_show(name: &str) -> ExitCode {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut visited = Vec::new();
    let mut profile = match resolve(name, &mut visited, 0) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e:#}");
            return ExitCode::from(1);
        }
    };

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let tmpdir = std::env::temp_dir();
    expand_profile(&mut profile, &home, &cwd, &tmpdir);

    match serde_json::to_string_pretty(&profile) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_use_string() {
        let p: Profile = serde_json::from_str(r#"{"use": "hardened"}"#).unwrap();
        assert_eq!(p.uses, vec!["hardened"]);
    }

    #[test]
    fn deserialize_use_array() {
        let p: Profile = serde_json::from_str(r#"{"use": ["a", "b"]}"#).unwrap();
        assert_eq!(p.uses, vec!["a", "b"]);
    }

    #[test]
    fn deserialize_empty_profile() {
        let p: Profile = serde_json::from_str("{}").unwrap();
        assert!(p.uses.is_empty());
        assert!(p.allow_read.is_none());
    }

    #[test]
    fn rejects_unknown_fields() {
        let result = serde_json::from_str::<Profile>(r#"{"bad_field": true}"#);
        assert!(result.is_err());
    }

    #[test]
    fn template_expansion() {
        let result = expand_templates(
            "$HOME/.config",
            Path::new("/users/me"),
            Path::new("/proj"),
            Path::new("/tmp"),
        );
        assert_eq!(result, "/users/me/.config");
    }

    #[test]
    fn merge_vecs_dedup_append() {
        let base = Some(vec!["a".to_string(), "b".to_string()]);
        let child = Some(vec!["b".to_string(), "c".to_string()]);
        let merged = dedup_append(&base, &child);
        assert_eq!(
            merged,
            Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
    }

    #[test]
    fn merge_vecs_none_inherits() {
        let base = Some(vec!["a".to_string()]);
        let merged = dedup_append(&base, &None);
        assert_eq!(merged, Some(vec!["a".to_string()]));
    }

    #[test]
    fn merge_bool_child_wins() {
        let base = Profile {
            strict_sandbox: Some(true),
            ..Default::default()
        };
        let child = Profile {
            strict_sandbox: Some(false),
            ..Default::default()
        };
        let merged = merge_profiles(&base, &child);
        assert_eq!(merged.strict_sandbox, Some(false));
    }

    #[test]
    fn merge_bool_inherits_when_none() {
        let base = Profile {
            strict_sandbox: Some(true),
            ..Default::default()
        };
        let child = Profile::default();
        let merged = merge_profiles(&base, &child);
        assert_eq!(merged.strict_sandbox, Some(true));
    }

    #[test]
    fn merge_env_maps() {
        let base = Profile {
            set_env: Some(HashMap::from([
                ("A".into(), "1".into()),
                ("B".into(), "2".into()),
            ])),
            ..Default::default()
        };
        let child = Profile {
            set_env: Some(HashMap::from([
                ("A".into(), "override".into()),
                ("C".into(), "3".into()),
            ])),
            ..Default::default()
        };
        let merged = merge_profiles(&base, &child);
        let env = merged.set_env.unwrap();
        assert_eq!(env["A"], "override");
        assert_eq!(env["B"], "2");
        assert_eq!(env["C"], "3");
    }

    #[test]
    fn builtin_profiles_parse() {
        for (name, json) in BUILTIN_PROFILES {
            let result = serde_json::from_str::<Profile>(json);
            assert!(
                result.is_ok(),
                "built-in profile '{name}' failed to parse: {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn resolve_builtin_hardened() {
        let mut visited = Vec::new();
        let profile = resolve("hardened", &mut visited, 0).unwrap();
        assert_eq!(profile.strict_sandbox, Some(true));
        assert!(profile.deny_read.is_some());
    }

    #[test]
    fn resolve_builtin_ai_agent_uses_hardened() {
        let mut visited = Vec::new();
        let profile = resolve("ai-agent", &mut visited, 0).unwrap();
        assert_eq!(profile.strict_sandbox, Some(true));
        assert!(profile.deny_read.is_some());
        assert!(profile.allow_write.is_some());
    }

    #[test]
    fn platform_filtering_skips_non_matching() {
        let mut visited = Vec::new();
        let profile = resolve("claude-code", &mut visited, 0).unwrap();
        // On any platform, should resolve without error.
        // Platform-specific configs are silently skipped on non-matching OS.
        assert!(profile.allow_net.is_some());
    }

    #[test]
    fn circular_inheritance_detected() {
        // Can't test with built-ins, but test the mechanism.
        let mut visited = vec!["a".to_string()];
        let result = resolve("a", &mut visited, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("circular"));
    }
}
