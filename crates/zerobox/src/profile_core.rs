use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

const MAX_INHERITANCE_DEPTH: usize = 100;

const BUILTIN_PROFILES: &[(&str, &str)] = &[
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
        "linux-sysfs-read",
        include_str!("../profiles/linux-sysfs-read.json"),
    ),
    ("cache-macos", include_str!("../profiles/cache-macos.json")),
    ("cache-linux", include_str!("../profiles/cache-linux.json")),
    (
        "claude-cache-linux",
        include_str!("../profiles/claude-cache-linux.json"),
    ),
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
    (
        "homebrew-macos",
        include_str!("../profiles/homebrew-macos.json"),
    ),
    (
        "homebrew-linux",
        include_str!("../profiles/homebrew-linux.json"),
    ),
    ("go-runtime", include_str!("../profiles/go-runtime.json")),
    ("nix-runtime", include_str!("../profiles/nix-runtime.json")),
    ("user-tools", include_str!("../profiles/user-tools.json")),
    ("git-config", include_str!("../profiles/git-config.json")),
    (
        "claude-macos",
        include_str!("../profiles/claude-macos.json"),
    ),
    (
        "claude-linux",
        include_str!("../profiles/claude-linux.json"),
    ),
    ("codex-macos", include_str!("../profiles/codex-macos.json")),
    (
        "opencode-linux",
        include_str!("../profiles/opencode-linux.json"),
    ),
    ("default", include_str!("../profiles/default.json")),
    ("workspace", include_str!("../profiles/workspace.json")),
    ("claude", include_str!("../profiles/claude.json")),
    ("codex", include_str!("../profiles/codex.json")),
    ("opencode", include_str!("../profiles/opencode.json")),
    ("openclaw", include_str!("../profiles/openclaw.json")),
];

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    #[schemars(rename = "$schema")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, rename = "use", deserialize_with = "deserialize_use")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[schemars(rename = "use", schema_with = "schema_string_or_array")]
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

fn schema_string_or_array(_: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
    serde_json::from_value(serde_json::json!({
        "description": "Profile(s) to compose from.",
        "oneOf": [
            { "type": "string" },
            { "type": "array", "items": { "type": "string" } }
        ]
    }))
    .unwrap()
}

fn deserialize_use<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        Single(String),
        Multiple(Vec<String>),
    }
    match StringOrVec::deserialize(d)? {
        StringOrVec::Single(s) => Ok(vec![s]),
        StringOrVec::Multiple(v) => Ok(v),
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

pub fn expand_profile(p: &mut Profile, home: &Path, cwd: &Path, tmpdir: &Path) {
    expand_vec(&mut p.allow_read, home, cwd, tmpdir);
    expand_vec(&mut p.deny_read, home, cwd, tmpdir);
    expand_vec(&mut p.allow_write, home, cwd, tmpdir);
    expand_vec(&mut p.deny_write, home, cwd, tmpdir);
    expand_vec(&mut p.snapshot_paths, home, cwd, tmpdir);
    expand_vec(&mut p.snapshot_exclude, home, cwd, tmpdir);
    if let Some(ref mut env) = p.set_env {
        for val in env.values_mut() {
            *val = expand_templates(val, home, cwd, tmpdir);
        }
    }
}

pub fn dedup_append(
    base: &Option<Vec<String>>,
    child: &Option<Vec<String>>,
) -> Option<Vec<String>> {
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

pub fn merge_profiles(base: &Profile, child: &Profile) -> Profile {
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

fn validate_profile_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
        && !name.contains("..")
}

fn load_raw(name: &str) -> Result<Profile> {
    if !validate_profile_name(name) {
        bail!("invalid profile name: '{name}'");
    }

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

pub fn resolve(name: &str, chain: &mut Vec<String>, depth: usize) -> Result<Profile> {
    if depth > MAX_INHERITANCE_DEPTH {
        bail!("profile composition too deep (max {MAX_INHERITANCE_DEPTH})");
    }
    if chain.contains(&name.to_string()) {
        chain.push(name.to_string());
        bail!("circular profile reference: {}", chain.join(" -> "));
    }
    chain.push(name.to_string());

    let profile = load_raw(name)?;

    let result = if !platform_matches(&profile) {
        Ok(Profile::default())
    } else if profile.uses.is_empty() {
        Ok(profile)
    } else {
        let mut merged = Profile::default();
        for dep_name in &profile.uses {
            let dep = resolve(dep_name, chain, depth + 1)?;
            merged = merge_profiles(&merged, &dep);
        }
        Ok(merge_profiles(&merged, &profile))
    };

    chain.pop();
    result
}

pub fn load_profile(name: &str, cwd: &Path) -> Result<Profile> {
    let mut chain = Vec::new();
    let mut profile = resolve(name, &mut chain, 0)?;
    expand_with_env(&mut profile, cwd);
    Ok(profile)
}

/// Resolve a list of profile names and merge them left-to-right. Later
/// entries override earlier ones on conflicting fields, matching the
/// semantics of `use:` composition. Single-element input is equivalent
/// to `load_profile`.
pub fn load_profiles<S: AsRef<str>>(names: &[S], cwd: &Path) -> Result<Profile> {
    if let [single] = names {
        return load_profile(single.as_ref(), cwd);
    }
    let mut merged = Profile::default();
    for name in names {
        let mut chain = Vec::new();
        let resolved = resolve(name.as_ref(), &mut chain, 0)?;
        merged = merge_profiles(&merged, &resolved);
    }
    expand_with_env(&mut merged, cwd);
    Ok(merged)
}

fn expand_with_env(profile: &mut Profile, cwd: &Path) {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let tmpdir = std::env::temp_dir();
    expand_profile(profile, &home, cwd, &tmpdir);
}

pub fn builtin_profiles() -> &'static [(&'static str, &'static str)] {
    BUILTIN_PROFILES
}

/// Raw `use:` list from a profile, without resolving composition.
pub fn load_profile_uses(name: &str) -> Option<Vec<String>> {
    load_raw(name).ok().map(|p| p.uses)
}
