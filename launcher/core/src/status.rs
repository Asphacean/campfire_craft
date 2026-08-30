//! D-05/LNCH-07: `GET /status`, polled by the frontend on start and every
//! 15 seconds. Any failure at all — timeout, DNS, TLS, a malformed body —
//! maps to the offline state rather than an error: the pill must be able
//! to show "offline" even when the *service* (not just the game) is
//! unreachable, while the log can still tell the two cases apart.

use serde::{Deserialize, Serialize};

use crate::http::{campfire_base_url, campfire_client};
use crate::log;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    pub online: bool,
    pub players: Option<u32>,
    pub max: Option<u32>,
    pub motd: Option<String>,
}

impl ServerStatus {
    fn offline() -> Self {
        Self {
            online: false,
            players: None,
            max: None,
            motd: None,
        }
    }
}

/// Never returns `Err` to the caller — an unreachable service is reported
/// as the offline state, exactly like an unreachable game server. The log
/// still records *why* (network vs. a genuinely offline game), which a
/// bare `online: false` in the UI cannot distinguish.
pub async fn fetch_status() -> ServerStatus {
    let resp = campfire_client()
        .get(format!("{}/status", campfire_base_url()))
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            log::info(&format!("status: request failed (service unreachable): {e}"));
            return ServerStatus::offline();
        }
    };

    match resp.json::<ServerStatus>().await {
        Ok(status) => status,
        Err(e) => {
            log::info(&format!("status: malformed response body: {e}"));
            ServerStatus::offline()
        }
    }
}
