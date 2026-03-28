/// Thin shim for codex-network-proxy. Only provides the types and functions
/// that `codex-sandboxing` actually imports.
use std::collections::HashMap;
use std::net::SocketAddr;

pub const PROXY_URL_ENV_KEYS: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "WS_PROXY",
    "WSS_PROXY",
    "ALL_PROXY",
    "FTP_PROXY",
    "YARN_HTTP_PROXY",
    "YARN_HTTPS_PROXY",
    "NPM_CONFIG_HTTP_PROXY",
    "NPM_CONFIG_HTTPS_PROXY",
    "NPM_CONFIG_PROXY",
    "BUNDLE_HTTP_PROXY",
    "BUNDLE_HTTPS_PROXY",
    "PIP_PROXY",
    "DOCKER_HTTP_PROXY",
    "DOCKER_HTTPS_PROXY",
];

pub const ALL_PROXY_ENV_KEYS: &[&str] = &["ALL_PROXY", "all_proxy"];
pub const ALLOW_LOCAL_BINDING_ENV_KEY: &str = "CODEX_NETWORK_ALLOW_LOCAL_BINDING";
pub const NO_PROXY_ENV_KEYS: &[&str] = &[
    "NO_PROXY",
    "no_proxy",
    "npm_config_noproxy",
    "NPM_CONFIG_NOPROXY",
    "YARN_NO_PROXY",
    "BUNDLE_NO_PROXY",
];
pub const DEFAULT_NO_PROXY_VALUE: &str = concat!(
    "localhost,127.0.0.1,::1,",
    "*.local,.local,",
    "169.254.0.0/16,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16"
);

pub fn proxy_url_env_value<'a>(
    env: &'a HashMap<String, String>,
    canonical_key: &str,
) -> Option<&'a str> {
    if let Some(value) = env.get(canonical_key) {
        return Some(value.as_str());
    }
    let lower_key = canonical_key.to_ascii_lowercase();
    env.get(lower_key.as_str()).map(String::as_str)
}

pub fn has_proxy_url_env_vars(env: &HashMap<String, String>) -> bool {
    PROXY_URL_ENV_KEYS
        .iter()
        .any(|key| proxy_url_env_value(env, key).is_some_and(|value| !value.trim().is_empty()))
}

/// Minimal NetworkProxy shim. The sandboxing crate passes `Option<&NetworkProxy>`
/// and calls accessor methods on it. Fields are private; use the accessors.
#[derive(Clone, Debug)]
pub struct NetworkProxy {
    http_addr_val: SocketAddr,
    socks_addr_val: SocketAddr,
    socks_enabled_val: bool,
    allow_local_binding_val: bool,
    allow_unix_sockets_val: Vec<String>,
    dangerously_allow_all_unix_sockets_val: bool,
}

impl NetworkProxy {
    pub fn new(
        http_addr: SocketAddr,
        socks_addr: SocketAddr,
        socks_enabled: bool,
        allow_local_binding: bool,
        allow_unix_sockets: Vec<String>,
        dangerously_allow_all_unix_sockets: bool,
    ) -> Self {
        Self {
            http_addr_val: http_addr,
            socks_addr_val: socks_addr,
            socks_enabled_val: socks_enabled,
            allow_local_binding_val: allow_local_binding,
            allow_unix_sockets_val: allow_unix_sockets,
            dangerously_allow_all_unix_sockets_val: dangerously_allow_all_unix_sockets,
        }
    }

    pub fn http_addr(&self) -> SocketAddr {
        self.http_addr_val
    }

    pub fn socks_addr(&self) -> SocketAddr {
        self.socks_addr_val
    }

    pub fn allow_local_binding(&self) -> bool {
        self.allow_local_binding_val
    }

    pub fn allow_unix_sockets(&self) -> &[String] {
        &self.allow_unix_sockets_val
    }

    pub fn dangerously_allow_all_unix_sockets(&self) -> bool {
        self.dangerously_allow_all_unix_sockets_val
    }

    pub fn apply_to_env(&self, env: &mut HashMap<String, String>) {
        let http_url = format!("http://{}", self.http_addr_val);

        // ALL_PROXY gets SOCKS URL when SOCKS is enabled, HTTP URL otherwise.
        let all_proxy_url = if self.socks_enabled_val {
            format!("socks5h://{}", self.socks_addr_val)
        } else {
            http_url.clone()
        };

        fn set_both_cases(env: &mut HashMap<String, String>, key: &str, value: &str) {
            env.insert(key.to_string(), value.to_string());
            let lower = key.to_ascii_lowercase();
            if lower != key {
                env.insert(lower, value.to_string());
            }
        }

        for key in &["HTTP_PROXY", "HTTPS_PROXY"] {
            set_both_cases(env, key, &http_url);
        }

        set_both_cases(env, "ALL_PROXY", &all_proxy_url);

        // WebSocket keys get SOCKS URL when enabled, HTTP otherwise.
        for key in &["WS_PROXY", "WSS_PROXY"] {
            set_both_cases(env, key, &all_proxy_url);
        }

        for key in &[
            "FTP_PROXY",
            "YARN_HTTP_PROXY",
            "YARN_HTTPS_PROXY",
            "NPM_CONFIG_HTTP_PROXY",
            "NPM_CONFIG_HTTPS_PROXY",
            "NPM_CONFIG_PROXY",
            "BUNDLE_HTTP_PROXY",
            "BUNDLE_HTTPS_PROXY",
            "PIP_PROXY",
            "DOCKER_HTTP_PROXY",
            "DOCKER_HTTPS_PROXY",
        ] {
            set_both_cases(env, key, &http_url);
        }

        for key in NO_PROXY_ENV_KEYS {
            env.insert((*key).to_string(), DEFAULT_NO_PROXY_VALUE.to_string());
        }

        if self.allow_local_binding_val {
            env.insert(ALLOW_LOCAL_BINDING_ENV_KEY.to_string(), "1".to_string());
        }
    }
}
