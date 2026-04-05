use std::collections::{HashMap, HashSet};

use crate::Cli;

/// Minimal env vars inherited by default (no --allow-env flag).
const DEFAULT_ENV_KEYS: &[&str] = &["PATH", "HOME", "USER", "SHELL", "TERM", "LANG"];

/// Build the base environment for the sandboxed child process.
///
/// Default: clean env with only essential vars (PATH, HOME, etc.).
/// --allow-env (bare): inherit all parent env vars.
/// --allow-env=K1,K2: inherit only listed keys.
/// --deny-env=K1,K2: drop listed keys (takes precedence).
/// --env KEY=VALUE: always inserted regardless of filtering.
pub fn build_child_env(cli: &Cli) -> Result<HashMap<String, String>, String> {
    let parent_env: HashMap<String, String> = std::env::vars().collect();

    let mut env = match &cli.allow_env {
        // --allow-env (bare, empty list): inherit all.
        Some(keys) if keys.is_empty() => parent_env,
        // --allow-env=K1,K2: inherit only listed keys.
        Some(keys) => {
            let allow_set: HashSet<&str> = keys.iter().map(|s| s.as_str()).collect();
            parent_env
                .into_iter()
                .filter(|(k, _)| allow_set.contains(k.as_str()))
                .collect()
        }
        // No --allow-env flag: clean env with only essentials.
        None => parent_env
            .into_iter()
            .filter(|(k, _)| DEFAULT_ENV_KEYS.contains(&k.as_str()))
            .collect(),
    };

    if let Some(ref denied) = cli.deny_env {
        for key in denied {
            env.remove(key);
        }
    }

    for pair in &cli.set_env {
        if let Some((key, value)) = pair.split_once('=') {
            env.insert(key.to_string(), value.to_string());
        } else {
            return Err(format!(
                "invalid --env value '{pair}': expected KEY=VALUE format"
            ));
        }
    }

    Ok(env)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli_with(
        set_env: Vec<&str>,
        allow_env: Option<Vec<&str>>,
        deny_env: Option<Vec<&str>>,
    ) -> Cli {
        Cli {
            allow_read: None,
            deny_read: None,
            allow_write: None,
            deny_write: None,
            allow_net: None,
            deny_net: None,
            allow_all: false,
            cwd: None,
            no_sandbox: false,
            set_env: set_env.into_iter().map(String::from).collect(),
            allow_env: allow_env.map(|v| v.into_iter().map(String::from).collect()),
            deny_env: deny_env.map(|v| v.into_iter().map(String::from).collect()),
            secret: vec![],
            secret_host: vec![],
            strict_sandbox: false,
            debug: false,
            snapshot: false,
            restore: false,
            snapshot_paths: None,
            snapshot_exclude: None,
            profile: None,
            subcommand: None,
            command: vec!["true".to_string()],
        }
    }

    #[test]
    fn default_only_essential_keys() {
        let env = build_child_env(&cli_with(vec![], None, None)).unwrap();
        for key in DEFAULT_ENV_KEYS {
            if std::env::var(key).is_ok() {
                assert!(env.contains_key(*key), "expected {key} in default env");
            }
        }
        // Should not contain random parent vars.
        assert!(!env.contains_key("CARGO_MANIFEST_DIR"));
    }

    #[test]
    fn allow_env_bare_inherits_all() {
        let env = build_child_env(&cli_with(vec![], Some(vec![]), None)).unwrap();
        // Should have many more than the default set.
        assert!(env.len() > DEFAULT_ENV_KEYS.len());
    }

    #[test]
    fn allow_env_specific_keys() {
        let env = build_child_env(&cli_with(vec![], Some(vec!["PATH"]), None)).unwrap();
        assert!(env.contains_key("PATH"));
        assert!(!env.contains_key("HOME"));
    }

    #[test]
    fn deny_env_removes_key() {
        let env = build_child_env(&cli_with(vec![], Some(vec![]), Some(vec!["HOME"]))).unwrap();
        assert!(!env.contains_key("HOME"));
    }

    #[test]
    fn deny_takes_precedence_over_allow() {
        let env = build_child_env(&cli_with(
            vec![],
            Some(vec!["PATH", "HOME"]),
            Some(vec!["HOME"]),
        ))
        .unwrap();
        assert!(env.contains_key("PATH"));
        assert!(!env.contains_key("HOME"));
    }

    #[test]
    fn set_env_inserts_var() {
        let env = build_child_env(&cli_with(vec!["FOO=bar"], None, None)).unwrap();
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn set_env_survives_deny() {
        let env =
            build_child_env(&cli_with(vec!["FOO=bar"], Some(vec![]), Some(vec!["FOO"]))).unwrap();
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn set_env_survives_allow_filter() {
        let env = build_child_env(&cli_with(vec!["FOO=bar"], Some(vec!["PATH"]), None)).unwrap();
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn set_env_value_with_equals() {
        let env = build_child_env(&cli_with(vec!["DATA=a=b=c"], None, None)).unwrap();
        assert_eq!(env.get("DATA"), Some(&"a=b=c".to_string()));
    }

    #[test]
    fn set_env_empty_value() {
        let env = build_child_env(&cli_with(vec!["EMPTY="], None, None)).unwrap();
        assert_eq!(env.get("EMPTY"), Some(&String::new()));
    }

    #[test]
    fn set_env_no_equals_is_error() {
        let result = build_child_env(&cli_with(vec!["BADFORMAT"], None, None));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("KEY=VALUE"));
    }

    #[test]
    fn deny_env_on_default_clean_env() {
        // deny-env without allow-env should remove from the default essentials.
        let env = build_child_env(&cli_with(vec![], None, Some(vec!["PATH"]))).unwrap();
        assert!(!env.contains_key("PATH"), "PATH should be denied");
        // Other essentials still present.
        if std::env::var("HOME").is_ok() {
            assert!(env.contains_key("HOME"), "HOME should survive");
        }
    }

    #[test]
    fn deny_env_nonexistent_key_is_noop() {
        let env = build_child_env(&cli_with(
            vec![],
            Some(vec![]),
            Some(vec!["DEFINITELY_NOT_SET_ZEROBOX_TEST"]),
        ))
        .unwrap();
        // Should succeed without error — denying a non-existent key is fine.
        assert!(!env.contains_key("DEFINITELY_NOT_SET_ZEROBOX_TEST"));
    }
}
