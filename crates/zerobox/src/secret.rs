use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::path::PathBuf;

/// A single secret entry with its random placeholder, real value, and host restrictions.
/// Debug impl intentionally omits the real value.
pub struct SecretEntry {
    /// The environment variable name (e.g., "OPENAI_API_KEY").
    pub key: String,
    /// The random placeholder token visible to the sandboxed process.
    pub placeholder: String,
    /// The real secret value. Private — only exposed during header substitution.
    value: String,
    /// Host patterns this secret applies to. Empty = unrestricted (all hosts).
    pub hosts: Vec<String>,
}

impl std::fmt::Debug for SecretEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretEntry")
            .field("key", &self.key)
            .field("placeholder", &self.placeholder)
            .field("value", &"[REDACTED]")
            .field("hosts", &self.hosts)
            .finish()
    }
}

/// Stores all secret mappings. Shared with the proxy via Arc for header substitution.
#[derive(Default, Debug)]
pub struct SecretStore {
    entries: Vec<SecretEntry>,
}

/// Generate a random placeholder token: `ZEROBOX_SECRET_<64 hex chars>`.
/// Not derived from the secret value to prevent offline brute-forcing.
fn generate_placeholder() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("failed to generate random bytes");
    let mut hex = String::with_capacity(15 + 64); // "ZEROBOX_SECRET_" + 64 hex chars
    hex.push_str("ZEROBOX_SECRET_");
    for b in &bytes {
        write!(hex, "{b:02x}").expect("write to String cannot fail");
    }
    hex
}

/// Parse `--secret` and `--secret-host` CLI flags into a SecretStore.
///
/// - `secrets`: `["KEY=VALUE", ...]` from `--secret`
/// - `secret_hosts`: `["KEY=host1,host2", ...]` from `--secret-host`
pub fn parse_secret_flags(
    secrets: &[String],
    secret_hosts: &[String],
) -> Result<SecretStore, String> {
    let mut entries = Vec::new();
    let mut seen_keys = HashSet::new();

    for pair in secrets {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("invalid --secret value '{pair}': expected KEY=VALUE format"))?;
        if key.is_empty() {
            return Err(format!(
                "invalid --secret value '{pair}': key cannot be empty"
            ));
        }
        if !seen_keys.insert(key) {
            return Err(format!("duplicate --secret key '{key}'"));
        }
        entries.push(SecretEntry {
            key: key.to_string(),
            placeholder: generate_placeholder(),
            value: value.to_string(),
            hosts: Vec::new(),
        });
    }

    for host_spec in secret_hosts {
        let (key, hosts_str) = host_spec.split_once('=').ok_or_else(|| {
            format!("invalid --secret-host value '{host_spec}': expected KEY=host1,host2 format")
        })?;
        let entry = entries.iter_mut().find(|e| e.key == key).ok_or_else(|| {
            format!("--secret-host references unknown secret '{key}': define it with --secret {key}=<value> first")
        })?;
        entry
            .hosts
            .extend(hosts_str.split(',').map(|s| s.trim().to_ascii_lowercase()));
    }

    Ok(SecretStore { entries })
}

impl SecretStore {
    /// Returns true if there are no secrets.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns true if MITM is required (any secrets configured).
    pub fn requires_mitm(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Returns KEY=placeholder map for injecting into the child environment.
    pub fn get_env_overrides(&self) -> HashMap<String, String> {
        self.entries
            .iter()
            .map(|e| (e.key.clone(), e.placeholder.clone()))
            .collect()
    }

    /// Returns the union of all host patterns across all secrets.
    pub fn get_allowed_hosts(&self) -> Vec<String> {
        self.entries
            .iter()
            .flat_map(|e| e.hosts.iter().cloned())
            .collect()
    }

    /// Scan all header values and replace placeholder tokens with real values
    /// when the target host matches. Returns true if any substitution was made.
    pub fn substitute_headers(
        &self,
        headers: &mut rama_http::HeaderMap,
        target_host: &str,
    ) -> bool {
        if self.entries.is_empty() {
            return false;
        }

        let normalized_host = target_host.to_ascii_lowercase();
        let mut modified = false;
        let names: Vec<_> = headers.keys().cloned().collect();

        for name in &names {
            let values: Vec<rama_http::HeaderValue> =
                headers.get_all(name).iter().cloned().collect();
            headers.remove(name);

            for val in values {
                let val_str = match val.to_str() {
                    Ok(s) => s.to_string(),
                    Err(_) => {
                        // Non-UTF8 header value — re-insert unchanged.
                        headers.append(name, val);
                        continue;
                    }
                };

                let replaced = self.substitute_in_str(&val_str, &normalized_host);
                if replaced != val_str {
                    modified = true;
                }

                if let Ok(new_val) = rama_http::HeaderValue::from_str(&replaced) {
                    headers.append(name, new_val);
                } else {
                    // Substitution produced invalid header value — keep original.
                    headers.append(name, val);
                }
            }
        }

        modified
    }

    fn substitute_in_str<'a>(
        &self,
        input: &'a str,
        target_host: &str,
    ) -> std::borrow::Cow<'a, str> {
        let mut result: Option<String> = None;
        for entry in &self.entries {
            if !entry.host_matches(target_host) {
                continue;
            }
            let s = result.as_deref().unwrap_or(input);
            if s.contains(&entry.placeholder) {
                result = Some(s.replace(&entry.placeholder, &entry.value));
            }
        }
        match result {
            Some(s) => std::borrow::Cow::Owned(s),
            None => std::borrow::Cow::Borrowed(input),
        }
    }
}

impl SecretEntry {
    /// Check if `target_host` (must be pre-lowercased) matches this entry's host patterns.
    fn host_matches(&self, target_host: &str) -> bool {
        if self.hosts.is_empty() {
            return true; // No restriction — matches all hosts.
        }
        // Patterns are pre-normalized to lowercase at parse time.
        // Caller must pass target_host already lowercased.
        self.hosts.iter().any(|pattern| {
            if let Some(suffix) = pattern.strip_prefix("*.") {
                // Require a dot boundary: *.example.com matches sub.example.com
                // but NOT evilexample.com or example.com itself.
                target_host.ends_with(suffix)
                    && target_host.len() > suffix.len()
                    && target_host.as_bytes()[target_host.len() - suffix.len() - 1] == b'.'
            } else {
                target_host == pattern.as_str()
            }
        })
    }
}

/// Returns the MITM CA certificate path (`<zerobox_home>/proxy/ca.pem`) if it exists.
pub fn mitm_ca_cert_path() -> Option<PathBuf> {
    let path = crate::zerobox_home().join("proxy").join("ca.pem");
    if path.exists() { Some(path) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Placeholder generation ──

    #[test]
    fn generate_placeholder_format() {
        let p = generate_placeholder();
        assert!(p.starts_with("ZEROBOX_SECRET_"), "got: {p}");
        let hex = &p["ZEROBOX_SECRET_".len()..];
        assert_eq!(hex.len(), 64, "expected 64 hex chars, got {}", hex.len());
        assert!(
            hex.chars().all(|c| c.is_ascii_hexdigit()),
            "not all hex: {hex}"
        );
    }

    #[test]
    fn generate_placeholder_uniqueness() {
        let a = generate_placeholder();
        let b = generate_placeholder();
        assert_ne!(a, b);
    }

    // ── Parsing ──

    #[test]
    fn parse_single_secret() {
        let store = parse_secret_flags(&["API_KEY=sk-123".to_string()], &[]).unwrap();
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].key, "API_KEY");
        assert_eq!(store.entries[0].value, "sk-123");
        assert!(store.entries[0].placeholder.starts_with("ZEROBOX_SECRET_"));
        assert!(store.entries[0].hosts.is_empty());
    }

    #[test]
    fn parse_secret_with_equals_in_value() {
        let store = parse_secret_flags(&["KEY=a=b=c".to_string()], &[]).unwrap();
        assert_eq!(store.entries[0].value, "a=b=c");
    }

    #[test]
    fn parse_secret_no_equals_is_error() {
        let result = parse_secret_flags(&["BADFORMAT".to_string()], &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("KEY=VALUE"));
    }

    #[test]
    fn parse_secret_duplicate_key_is_error() {
        let result = parse_secret_flags(&["KEY=val1".to_string(), "KEY=val2".to_string()], &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("duplicate"));
    }

    #[test]
    fn parse_secret_empty_key_is_error() {
        let result = parse_secret_flags(&["=value".to_string()], &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("key cannot be empty"));
    }

    #[test]
    fn parse_secret_empty_value() {
        let store = parse_secret_flags(&["KEY=".to_string()], &[]).unwrap();
        assert_eq!(store.entries[0].value, "");
    }

    #[test]
    fn parse_secret_host_basic() {
        let store = parse_secret_flags(
            &["KEY=val".to_string()],
            &["KEY=api.example.com,other.com".to_string()],
        )
        .unwrap();
        assert_eq!(store.entries[0].hosts, vec!["api.example.com", "other.com"]);
    }

    #[test]
    fn parse_secret_host_unknown_key_is_error() {
        let result =
            parse_secret_flags(&["KEY=val".to_string()], &["UNKNOWN=host.com".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown secret"));
    }

    #[test]
    fn parse_secret_host_no_equals_is_error() {
        let result = parse_secret_flags(&["KEY=val".to_string()], &["BADFORMAT".to_string()]);
        assert!(result.is_err());
    }

    // ── Host matching ──

    fn entry(hosts: &[&str]) -> SecretEntry {
        SecretEntry {
            key: "K".into(),
            placeholder: "P".into(),
            value: "V".into(),
            // Pre-normalize to lowercase like parse_secret_flags does.
            hosts: hosts.iter().map(|s| s.to_ascii_lowercase()).collect(),
        }
    }

    #[test]
    fn host_matches_exact() {
        assert!(entry(&["api.example.com"]).host_matches("api.example.com"));
    }

    #[test]
    fn host_matches_case_insensitive() {
        // Patterns are lowercased at parse time via entry() helper.
        assert!(entry(&["API.Example.COM"]).host_matches("api.example.com"));
        // target_host must be pre-lowercased by caller (substitute_headers does this).
        assert!(entry(&["api.example.com"]).host_matches("api.example.com"));
    }

    #[test]
    fn host_matches_no_match() {
        assert!(!entry(&["api.example.com"]).host_matches("other.com"));
    }

    #[test]
    fn host_matches_unrestricted() {
        assert!(entry(&[]).host_matches("anything.com"));
        assert!(entry(&[]).host_matches("192.168.1.1"));
    }

    #[test]
    fn host_matches_wildcard() {
        assert!(entry(&["*.example.com"]).host_matches("api.example.com"));
        assert!(entry(&["*.example.com"]).host_matches("deep.sub.example.com"));
    }

    #[test]
    fn host_matches_wildcard_no_apex() {
        // *.example.com should NOT match example.com itself
        assert!(!entry(&["*.example.com"]).host_matches("example.com"));
    }

    #[test]
    fn host_matches_wildcard_requires_dot_boundary() {
        // *.example.com should NOT match evilexample.com
        assert!(!entry(&["*.example.com"]).host_matches("evilexample.com"));
        assert!(!entry(&["*.example.com"]).host_matches("notexample.com"));
        // But should match sub.example.com
        assert!(entry(&["*.example.com"]).host_matches("sub.example.com"));
    }

    #[test]
    fn host_matches_ip_address() {
        assert!(entry(&["93.184.216.34"]).host_matches("93.184.216.34"));
        assert!(!entry(&["93.184.216.34"]).host_matches("93.184.216.35"));
    }

    // ── Header substitution ──

    fn store_with(placeholder: &str, value: &str, hosts: &[&str]) -> SecretStore {
        SecretStore {
            entries: vec![SecretEntry {
                key: "K".into(),
                placeholder: placeholder.into(),
                value: value.into(),
                hosts: hosts.iter().map(|s| s.to_string()).collect(),
            }],
        }
    }

    #[test]
    fn substitute_matching_host() {
        let store = store_with("PLACEHOLDER", "real-secret", &["api.example.com"]);
        let mut headers = rama_http::HeaderMap::new();
        headers.insert("authorization", "Bearer PLACEHOLDER".parse().unwrap());

        let modified = store.substitute_headers(&mut headers, "api.example.com");
        assert!(modified);
        assert_eq!(headers.get("authorization").unwrap(), "Bearer real-secret");
    }

    #[test]
    fn substitute_non_matching_host() {
        let store = store_with("PLACEHOLDER", "real-secret", &["api.example.com"]);
        let mut headers = rama_http::HeaderMap::new();
        headers.insert("authorization", "Bearer PLACEHOLDER".parse().unwrap());

        let modified = store.substitute_headers(&mut headers, "evil.com");
        assert!(!modified);
        assert_eq!(headers.get("authorization").unwrap(), "Bearer PLACEHOLDER");
    }

    #[test]
    fn substitute_unrestricted_secret() {
        let store = store_with("PLACEHOLDER", "real-secret", &[]);
        let mut headers = rama_http::HeaderMap::new();
        headers.insert("authorization", "Bearer PLACEHOLDER".parse().unwrap());

        let modified = store.substitute_headers(&mut headers, "any-host.com");
        assert!(modified);
        assert_eq!(headers.get("authorization").unwrap(), "Bearer real-secret");
    }

    #[test]
    fn substitute_multiple_headers() {
        let store = store_with("PH", "secret", &[]);
        let mut headers = rama_http::HeaderMap::new();
        headers.insert("authorization", "Bearer PH".parse().unwrap());
        headers.insert("x-api-key", "PH".parse().unwrap());

        let modified = store.substitute_headers(&mut headers, "host.com");
        assert!(modified);
        assert_eq!(headers.get("authorization").unwrap(), "Bearer secret");
        assert_eq!(headers.get("x-api-key").unwrap(), "secret");
    }

    #[test]
    fn substitute_multiple_occurrences_in_one_value() {
        let store = store_with("PH", "s", &[]);
        let mut headers = rama_http::HeaderMap::new();
        headers.insert("x-data", "PH-and-PH".parse().unwrap());

        store.substitute_headers(&mut headers, "host.com");
        assert_eq!(headers.get("x-data").unwrap(), "s-and-s");
    }

    #[test]
    fn substitute_preserves_unrelated_headers() {
        let store = store_with("PH", "secret", &[]);
        let mut headers = rama_http::HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("authorization", "Bearer PH".parse().unwrap());

        store.substitute_headers(&mut headers, "host.com");
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
    }

    #[test]
    fn substitute_wildcard_host() {
        let store = store_with("PH", "secret", &["*.example.com"]);
        let mut headers = rama_http::HeaderMap::new();
        headers.insert("auth", "PH".parse().unwrap());

        let modified = store.substitute_headers(&mut headers, "api.example.com");
        assert!(modified);
        assert_eq!(headers.get("auth").unwrap(), "secret");
    }

    #[test]
    fn empty_store_is_noop() {
        let store = SecretStore::default();
        let mut headers = rama_http::HeaderMap::new();
        headers.insert("auth", "value".parse().unwrap());

        let modified = store.substitute_headers(&mut headers, "host.com");
        assert!(!modified);
    }

    // ── Env overrides and allowed hosts ──

    #[test]
    fn get_env_overrides() {
        let store = parse_secret_flags(&["A=val1".to_string(), "B=val2".to_string()], &[]).unwrap();
        let overrides = store.get_env_overrides();
        assert_eq!(overrides.len(), 2);
        assert!(overrides["A"].starts_with("ZEROBOX_SECRET_"));
        assert!(overrides["B"].starts_with("ZEROBOX_SECRET_"));
        assert_ne!(overrides["A"], overrides["B"]);
    }

    #[test]
    fn get_allowed_hosts() {
        let store = parse_secret_flags(
            &["A=v1".to_string(), "B=v2".to_string()],
            &["A=api.com".to_string(), "B=other.com,third.com".to_string()],
        )
        .unwrap();
        let hosts = store.get_allowed_hosts();
        assert_eq!(hosts, vec!["api.com", "other.com", "third.com"]);
    }

    #[test]
    fn get_allowed_hosts_empty_when_no_hosts() {
        let store = parse_secret_flags(&["A=v1".to_string()], &[]).unwrap();
        assert!(store.get_allowed_hosts().is_empty());
    }

    // ── Multiple secrets with different hosts ──

    #[test]
    fn substitute_two_secrets_different_hosts() {
        let store = SecretStore {
            entries: vec![
                SecretEntry {
                    key: "A".into(),
                    placeholder: "PH_A".into(),
                    value: "secret_a".into(),
                    hosts: vec!["host-a.com".into()],
                },
                SecretEntry {
                    key: "B".into(),
                    placeholder: "PH_B".into(),
                    value: "secret_b".into(),
                    hosts: vec!["host-b.com".into()],
                },
            ],
        };

        // Request to host-a.com: only PH_A substituted.
        let mut headers = rama_http::HeaderMap::new();
        headers.insert("x-key-a", "PH_A".parse().unwrap());
        headers.insert("x-key-b", "PH_B".parse().unwrap());
        store.substitute_headers(&mut headers, "host-a.com");
        assert_eq!(headers.get("x-key-a").unwrap(), "secret_a");
        assert_eq!(headers.get("x-key-b").unwrap(), "PH_B");

        // Request to host-b.com: only PH_B substituted.
        let mut headers = rama_http::HeaderMap::new();
        headers.insert("x-key-a", "PH_A".parse().unwrap());
        headers.insert("x-key-b", "PH_B".parse().unwrap());
        store.substitute_headers(&mut headers, "host-b.com");
        assert_eq!(headers.get("x-key-a").unwrap(), "PH_A");
        assert_eq!(headers.get("x-key-b").unwrap(), "secret_b");
    }

    // ── secret-host accumulation ──

    #[test]
    fn parse_secret_host_accumulates() {
        let store = parse_secret_flags(
            &["KEY=val".to_string()],
            &[
                "KEY=host1.com".to_string(),
                "KEY=host2.com,host3.com".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(
            store.entries[0].hosts,
            vec!["host1.com", "host2.com", "host3.com"]
        );
    }

    // ── is_empty / requires_mitm ──

    #[test]
    fn empty_store_is_empty() {
        let store = SecretStore::default();
        assert!(store.is_empty());
        assert!(!store.requires_mitm());
    }

    #[test]
    fn non_empty_store_requires_mitm() {
        let store = parse_secret_flags(&["KEY=val".to_string()], &[]).unwrap();
        assert!(!store.is_empty());
        assert!(store.requires_mitm());
    }

    // ── Empty inputs ──

    #[test]
    fn parse_empty_inputs() {
        let store = parse_secret_flags(&[], &[]).unwrap();
        assert!(store.is_empty());
        assert!(store.get_env_overrides().is_empty());
        assert!(store.get_allowed_hosts().is_empty());
    }

    // ── Non-UTF8 and invalid header values ──

    #[test]
    fn substitute_non_utf8_header_preserved() {
        let store = store_with("PH", "secret", &[]);
        let mut headers = rama_http::HeaderMap::new();
        // Insert a header with bytes that aren't valid UTF-8.
        let binary_val = rama_http::HeaderValue::from_bytes(&[0x80, 0x81, 0x82]).unwrap();
        headers.insert("x-binary", binary_val.clone());
        headers.insert("x-normal", "PH".parse().unwrap());

        let modified = store.substitute_headers(&mut headers, "host.com");
        assert!(modified); // x-normal was substituted
        assert_eq!(headers.get("x-binary").unwrap(), binary_val); // binary preserved
        assert_eq!(headers.get("x-normal").unwrap(), "secret");
    }

    #[test]
    fn substitute_invalid_result_keeps_original() {
        // If the real secret value contains characters invalid for HTTP headers,
        // the original placeholder should be kept.
        let store = SecretStore {
            entries: vec![SecretEntry {
                key: "K".into(),
                placeholder: "PH".into(),
                value: "bad\nvalue".into(), // newline is invalid in header values
                hosts: vec![],
            }],
        };
        let mut headers = rama_http::HeaderMap::new();
        headers.insert("auth", "PH".parse().unwrap());

        let modified = store.substitute_headers(&mut headers, "host.com");
        // Substitution was attempted (placeholder matched and replaced in string)
        // but the result contained invalid header characters, so original is kept.
        // `modified` is true because the string replacement did occur, even though
        // the invalid result was discarded.
        assert_eq!(headers.get("auth").unwrap(), "PH");
        assert!(modified);
    }

    // ── Wildcard edge: bare "*" is not a catch-all ──

    #[test]
    fn host_matches_bare_star_is_not_wildcard() {
        // "*" without a dot is treated as an exact match, not a wildcard.
        // Use empty hosts for catch-all behavior.
        assert!(!entry(&["*"]).host_matches("example.com"));
        assert!(entry(&["*"]).host_matches("*")); // only matches literal "*"
    }

    // ── No placeholder in any header ──

    #[test]
    fn substitute_no_placeholder_anywhere() {
        let store = store_with("ZEROBOX_SECRET_abc123", "real", &[]);
        let mut headers = rama_http::HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("accept", "*/*".parse().unwrap());

        let modified = store.substitute_headers(&mut headers, "host.com");
        assert!(!modified);
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
        assert_eq!(headers.get("accept").unwrap(), "*/*");
    }
}
