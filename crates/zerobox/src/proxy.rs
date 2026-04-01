//! Network proxy: domain-level filtering via Codex network-proxy.

use std::sync::Arc;

use anyhow::Result;
use codex_network_proxy::{
    ConfigReloader, ConfigState, NetworkMode, NetworkProxy, NetworkProxyConfig, NetworkProxyState,
    RequestHeaderTransformer, build_config_state,
};

use crate::Cli;
use crate::secret::SecretStore;

/// A ConfigReloader that never reloads (static config for CLI use).
struct StaticReloader;

#[async_trait::async_trait]
impl ConfigReloader for StaticReloader {
    fn source_label(&self) -> String {
        "zerobox static config".to_string()
    }

    async fn maybe_reload(&self) -> anyhow::Result<Option<ConfigState>> {
        Ok(None)
    }

    async fn reload_now(&self) -> anyhow::Result<ConfigState> {
        Err(anyhow::anyhow!("static config does not support reload"))
    }
}

/// Implement the upstream trait so SecretStore can be used as a header transformer.
impl RequestHeaderTransformer for SecretStore {
    fn transform_headers(&self, headers: &mut rama_http::HeaderMap, target_host: &str) {
        self.substitute_headers(headers, target_host);
    }
}

/// Build a NetworkProxy when domain filtering, deny rules, or secrets are configured.
pub async fn build_network_proxy(
    cli: &Cli,
    secret_store: &Arc<SecretStore>,
    debug: bool,
) -> Result<Option<NetworkProxy>> {
    let has_secrets = !secret_store.is_empty();

    // Determine allowed domains: CLI --allow-net + secret hosts.
    let allow_domains = cli.allow_net.as_deref().unwrap_or(&[]);
    let secret_hosts = secret_store.get_allowed_hosts();

    let has_net = cli.allow_net.is_some() || has_secrets;
    if !has_net {
        return Ok(None); // Network not enabled.
    }

    let has_filters = !allow_domains.is_empty()
        || !secret_hosts.is_empty()
        || cli.deny_net.is_some()
        || has_secrets;
    if !has_filters {
        return Ok(None); // Full network, no filtering needed, no secrets.
    }

    let mut config = NetworkProxyConfig::default();
    config.network.enabled = true;

    // Bare --allow-net (empty vec) means "allow all domains".
    let bare_allow_net = cli.allow_net.as_ref().is_some_and(|v| v.is_empty());

    if bare_allow_net {
        config.network.allowed_domains = vec!["*".to_string()];
    } else {
        let mut all_domains: Vec<String> = allow_domains.to_vec();
        all_domains.extend(secret_hosts);
        if all_domains.is_empty() {
            config.network.allowed_domains = vec!["*".to_string()];
        } else {
            config.network.allowed_domains = all_domains;
        }
    }
    if let Some(deny) = &cli.deny_net {
        config.network.denied_domains = deny.clone();
    }

    // When secrets are configured, enable MITM so HTTPS headers can be inspected
    // and placeholders substituted with real values.
    if has_secrets {
        config.network.mitm = true;
        config.network.mode = NetworkMode::Full;
    }

    let state = build_config_state(
        config,
        codex_network_proxy::NetworkProxyConstraints::default(),
    )?;

    let mut proxy_state = NetworkProxyState::with_reloader(state, Arc::new(StaticReloader));

    if has_secrets {
        proxy_state.set_header_transformer(secret_store.clone());
    }
    if debug {
        proxy_state
            .set_blocked_request_observer(Some(crate::debug::blocked_observer()))
            .await;
    }

    let proxy = NetworkProxy::builder()
        .state(Arc::new(proxy_state))
        .managed_by_codex(true)
        .build()
        .await?;

    Ok(Some(proxy))
}
