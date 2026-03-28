//! Network proxy: domain-level filtering via Codex network-proxy.

use std::sync::Arc;

use anyhow::Result;
use codex_network_proxy::{
    ConfigReloader, ConfigState, NetworkProxy, NetworkProxyConfig, NetworkProxyState,
    build_config_state,
};

use crate::Cli;

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

/// Build a NetworkProxy when --allow-net has domain filters or --deny-net is used.
pub async fn build_network_proxy(cli: &Cli) -> Result<Option<NetworkProxy>> {
    let Some(allow_domains) = &cli.allow_net else {
        return Ok(None); // Network not enabled.
    };

    let has_filters = !allow_domains.is_empty() || cli.deny_net.is_some();
    if !has_filters {
        return Ok(None); // Full network, no filtering needed.
    }

    let mut config = NetworkProxyConfig::default();
    config.network.enabled = true;

    if allow_domains.is_empty() {
        // Bare --allow-net with --deny-net: allow everything except denied.
        config.network.allowed_domains = vec!["*".to_string()];
    } else {
        config.network.allowed_domains = allow_domains.clone();
    }
    if let Some(deny) = &cli.deny_net {
        config.network.denied_domains = deny.clone();
    }

    let state = build_config_state(
        config,
        codex_network_proxy::NetworkProxyConstraints::default(),
    )?;

    let proxy_state = Arc::new(NetworkProxyState::with_reloader(
        state,
        Arc::new(StaticReloader),
    ));

    let proxy = NetworkProxy::builder()
        .state(proxy_state)
        .managed_by_codex(true)
        .build()
        .await?;

    Ok(Some(proxy))
}
