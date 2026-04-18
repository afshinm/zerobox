use std::path::PathBuf;
use std::process::ExitCode;

use zerobox::profile_core::{Profile, builtin_profiles, load_profile};

use crate::ProfileAction;

pub fn handle_subcommand(action: &ProfileAction) -> ExitCode {
    match action {
        ProfileAction::List => cmd_list(),
        ProfileAction::Show { name } => cmd_show(name),
        ProfileAction::Schema => cmd_schema(),
    }
}

fn cmd_schema() -> ExitCode {
    let schema = schemars::schema_for!(Profile);
    match serde_json::to_string_pretty(&schema) {
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

    let user_dir = zerobox::zerobox_home().join("profiles");
    let user_names: std::collections::BTreeSet<String> = std::fs::read_dir(&user_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|e| {
            e.path()
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .collect();

    for (name, json) in builtin_profiles() {
        if user_names.contains(*name) {
            continue;
        }
        let desc = serde_json::from_str::<Profile>(json)
            .ok()
            .and_then(|p| p.description)
            .unwrap_or_default();
        let _ = writeln!(tw, "{name}\tbuilt-in\t{desc}");
    }

    for name in &user_names {
        let path = user_dir.join(format!("{name}.json"));
        let desc = std::fs::read_to_string(&path)
            .ok()
            .and_then(|json| serde_json::from_str::<Profile>(&json).ok())
            .and_then(|p| p.description)
            .unwrap_or_default();
        let _ = writeln!(tw, "{name}\tuser\t{desc}");
    }

    let _ = tw.flush();
    ExitCode::SUCCESS
}

fn cmd_show(name: &str) -> ExitCode {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match load_profile(name, &cwd) {
        Ok(profile) => match serde_json::to_string_pretty(&profile) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(1)
            }
        },
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zerobox::profile_core::{self, Profile, resolve};

    #[test]
    fn builtin_profiles_parse() {
        for (name, json) in super::builtin_profiles() {
            let result = serde_json::from_str::<Profile>(json);
            assert!(
                result.is_ok(),
                "built-in profile '{name}' failed to parse: {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn resolve_builtin_default_has_deny_rules() {
        let mut chain = Vec::new();
        let profile = resolve("default", &mut chain, 0).unwrap();
        assert!(profile.deny_read.is_some());
        assert!(profile.deny_write.is_some());
        assert!(profile.allow_read.is_some());
    }

    #[test]
    fn resolve_builtin_workspace_uses_default() {
        let mut chain = Vec::new();
        let profile = resolve("workspace", &mut chain, 0).unwrap();
        assert!(profile.deny_read.is_some());
        assert!(profile.allow_write.is_some());
        assert!(profile.allow_read.is_some());
    }

    #[test]
    fn platform_filtering_skips_non_matching() {
        let mut chain = Vec::new();
        let profile = resolve("claude", &mut chain, 0).unwrap();
        assert!(profile.allow_net.is_some());
    }

    #[test]
    fn circular_inheritance_detected() {
        let mut chain = vec!["a".to_string()];
        let result = resolve("a", &mut chain, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("circular"));
    }

    #[test]
    fn rejects_path_traversal_in_profile_name() {
        assert!(resolve("../../../etc", &mut Vec::new(), 0).is_err());
        assert!(resolve("foo/bar", &mut Vec::new(), 0).is_err());
    }

    #[test]
    fn merge_vecs_dedup_append() {
        let base = Some(vec!["a".to_string(), "b".to_string()]);
        let child = Some(vec!["b".to_string(), "c".to_string()]);
        let merged = profile_core::dedup_append(&base, &child);
        assert_eq!(
            merged,
            Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
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
        let merged = profile_core::merge_profiles(&base, &child);
        assert_eq!(merged.strict_sandbox, Some(false));
    }

    #[test]
    fn load_profiles_single_matches_load_profile() {
        // claude-macos has no `use:` and a `platform` field, so it catches
        // regressions where the single case falls into the merge loop and
        // loses metadata.
        let cwd = std::env::current_dir().unwrap();
        let single = profile_core::load_profile("claude-macos", &cwd).unwrap();
        let multi = profile_core::load_profiles(&["claude-macos"], &cwd).unwrap();
        assert_eq!(single.allow_read, multi.allow_read);
        assert_eq!(single.allow_write, multi.allow_write);
        assert_eq!(single.platform, multi.platform);
        assert_eq!(single.description, multi.description);
        assert_eq!(single.schema, multi.schema);
    }

    #[test]
    fn load_profiles_empty_list_returns_default() {
        let cwd = std::env::current_dir().unwrap();
        let empty: [&str; 0] = [];
        let merged = profile_core::load_profiles(&empty, &cwd).unwrap();
        assert!(merged.allow_read.is_none());
        assert!(merged.allow_write.is_none());
        assert!(merged.uses.is_empty());
    }

    #[test]
    fn load_profiles_merges_left_to_right() {
        let cwd = std::env::current_dir().unwrap();
        let merged = profile_core::load_profiles(&["workspace", "git-config"], &cwd).unwrap();
        let reads = merged.allow_read.as_ref().unwrap();
        assert!(
            reads.iter().any(|p| p.ends_with(".gitconfig")),
            "git-config paths missing from merged profile: {reads:?}"
        );
        assert!(
            reads
                .iter()
                .any(|p| p.starts_with("/bin") || p.starts_with("/usr")),
            "workspace-chain system paths missing from merged profile: {reads:?}"
        );
    }

    #[test]
    fn load_profiles_deduplicates_repeated_names() {
        let cwd = std::env::current_dir().unwrap();
        let once = profile_core::load_profiles(&["workspace"], &cwd).unwrap();
        let twice = profile_core::load_profiles(&["workspace", "workspace"], &cwd).unwrap();
        assert_eq!(once.allow_read, twice.allow_read);
        assert_eq!(once.allow_write, twice.allow_write);
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
        let merged = profile_core::merge_profiles(&base, &child);
        let env = merged.set_env.unwrap();
        assert_eq!(env["A"], "override");
        assert_eq!(env["B"], "2");
        assert_eq!(env["C"], "3");
    }
}
