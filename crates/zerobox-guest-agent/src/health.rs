use std::sync::Arc;

use anyhow::Result;

use zerobox_common::protocol::HealthResult;

use crate::AgentState;

/// Return agent health status and uptime.
pub async fn handle_health(state: Arc<AgentState>) -> Result<serde_json::Value> {
    let uptime = state.start_time.elapsed().as_secs();

    let result = HealthResult {
        ok: true,
        uptime_secs: uptime,
    };

    Ok(serde_json::to_value(result)?)
}
