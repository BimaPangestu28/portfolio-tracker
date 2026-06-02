//! In-memory state of the WhatsApp gateway connection.
//!
//! The gateway pushes status/QR updates here and polls for control commands.
//! State is intentionally ephemeral — a backend restart simply waits for the
//! gateway's next heartbeat to repopulate it.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A reported "connected" is downgraded to "connecting" if the gateway has not
/// sent a heartbeat within this window, so the UI never shows a false positive.
const STALE_AFTER: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WaStatus {
    Disconnected,
    Connecting,
    Qr,
    Connected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WaCommand {
    Restart,
    Logout,
}

/// The frontend-facing snapshot of the connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WaStatusView {
    pub status: WaStatus,
    pub qr: Option<String>,
    pub number: Option<String>,
}

#[derive(Debug)]
pub struct WaState {
    status: WaStatus,
    qr: Option<String>,
    number: Option<String>,
    pending_command: Option<WaCommand>,
    last_seen: Option<Instant>,
}

impl Default for WaState {
    fn default() -> Self {
        Self {
            status: WaStatus::Disconnected,
            qr: None,
            number: None,
            pending_command: None,
            last_seen: None,
        }
    }
}

impl WaState {
    /// Apply a state push from the gateway and refresh the heartbeat clock.
    pub fn apply_push(
        &mut self,
        status: WaStatus,
        qr: Option<String>,
        number: Option<String>,
        now: Instant,
    ) {
        self.status = status;
        self.qr = qr;
        self.number = number;
        self.last_seen = Some(now);
    }

    /// Queue a control command for the gateway to pick up on its next poll.
    pub fn set_command(&mut self, command: WaCommand) {
        self.pending_command = Some(command);
    }

    /// Return the pending command exactly once, clearing it (consume-once).
    pub fn take_command(&mut self) -> Option<WaCommand> {
        self.pending_command.take()
    }

    /// Snapshot for the frontend, downgrading a stale "connected" to "connecting".
    pub fn view(&self, now: Instant) -> WaStatusView {
        let stale = self
            .last_seen
            .map_or(true, |seen| now.duration_since(seen) > STALE_AFTER);
        let status = if stale && self.status == WaStatus::Connected {
            WaStatus::Connecting
        } else {
            self.status
        };
        WaStatusView {
            status,
            qr: self.qr.clone(),
            number: self.number.clone(),
        }
    }
}

pub type SharedWaState = Arc<Mutex<WaState>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_command_returns_once_then_none() {
        let mut state = WaState::default();
        state.set_command(WaCommand::Logout);
        assert_eq!(state.take_command(), Some(WaCommand::Logout));
        assert_eq!(state.take_command(), None);
    }

    #[test]
    fn apply_push_updates_the_view() {
        let mut state = WaState::default();
        let now = Instant::now();
        state.apply_push(WaStatus::Qr, Some("abc".into()), None, now);
        let view = state.view(now);
        assert_eq!(view.status, WaStatus::Qr);
        assert_eq!(view.qr.as_deref(), Some("abc"));
    }

    #[test]
    fn connected_downgrades_when_heartbeat_is_stale() {
        let mut state = WaState::default();
        let earlier = Instant::now();
        state.apply_push(WaStatus::Connected, None, Some("62812".into()), earlier);
        // Fresh heartbeat keeps it connected.
        assert_eq!(state.view(earlier).status, WaStatus::Connected);
        // A stale heartbeat downgrades it.
        let later = earlier + STALE_AFTER + Duration::from_secs(1);
        assert_eq!(state.view(later).status, WaStatus::Connecting);
    }
}
