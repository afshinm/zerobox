use std::sync::Arc;

use anyhow::Result;
use zerobox_network_proxy::{
    ConfigReloader, ConfigState, NetworkMode, NetworkProxy, NetworkProxyConfig, NetworkProxyState,
    RequestHeaderTransformer, build_config_state,
};

use crate::secret::SecretStore;

pub async fn build_proxy(
    allow_domains: Option<&[String]>,
    deny_domains: Option<&[String]>,
    secret_store: &Arc<SecretStore>,
) -> Result<Option<NetworkProxy>> {
    let has_secrets = !secret_store.is_empty();
    let has_net = allow_domains.is_some() || has_secrets;
    if !has_net {
        return Ok(None);
    }

    let allow = allow_domains.unwrap_or(&[]);
    let secret_hosts = secret_store.get_allowed_hosts();
    let has_filters = !allow.is_empty()
        || !secret_hosts.is_empty()
        || deny_domains.is_some_and(|d| !d.is_empty())
        || has_secrets;
    if !has_filters {
        return Ok(None);
    }

    let mut config = NetworkProxyConfig::default();
    config.network.enabled = true;

    let bare_allow = allow.is_empty() && allow_domains.is_some();
    if bare_allow {
        config.network.set_allowed_domains(vec!["*".to_string()]);
    } else {
        let mut all: Vec<String> = allow.to_vec();
        all.extend(secret_hosts);
        if all.is_empty() {
            config.network.set_allowed_domains(vec!["*".to_string()]);
        } else {
            config.network.set_allowed_domains(all);
        }
    }
    if let Some(deny) = deny_domains {
        config.network.set_denied_domains(deny.to_vec());
    }

    if has_secrets {
        config.network.mitm = true;
        config.network.mode = NetworkMode::Full;
    }

    let state = build_config_state(
        config,
        zerobox_network_proxy::NetworkProxyConstraints::default(),
    )?;

    let mut proxy_state = NetworkProxyState::with_reloader(state, Arc::new(StaticReloader));
    if has_secrets {
        proxy_state.set_header_transformer(secret_store.clone());
    }

    let proxy = NetworkProxy::builder()
        .state(Arc::new(proxy_state))
        .managed_by_codex(true)
        .build()
        .await?;

    Ok(Some(proxy))
}

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
