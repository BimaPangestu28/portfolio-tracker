//! Outbound WhatsApp message queue for the CS number. The dashboard reply
//! endpoint pushes; the CS gateway drains it via GET /cs/whatsapp/outbound.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct OutboundMsg {
    pub jid:  String,
    pub text: String,
}

pub type SharedOutbound = Arc<Mutex<VecDeque<OutboundMsg>>>;

pub fn new_queue() -> SharedOutbound {
    Arc::new(Mutex::new(VecDeque::new()))
}

/// Enqueue a message. Lock poisoning is recovered (never panics).
pub fn push(q: &SharedOutbound, jid: &str, text: &str) {
    let mut g = q.lock().unwrap_or_else(|p| p.into_inner());
    g.push_back(OutboundMsg { jid: jid.to_string(), text: text.to_string() });
}

/// Drain all pending messages (at-most-once delivery — removed when handed out).
pub fn drain(q: &SharedOutbound) -> Vec<OutboundMsg> {
    let mut g = q.lock().unwrap_or_else(|p| p.into_inner());
    g.drain(..).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_then_drain_preserves_order_and_empties() {
        let q = new_queue();
        push(&q, "a@x", "hi");
        push(&q, "b@x", "yo");
        let out = drain(&q);
        assert_eq!(out, vec![
            OutboundMsg { jid: "a@x".into(), text: "hi".into() },
            OutboundMsg { jid: "b@x".into(), text: "yo".into() },
        ]);
        assert!(drain(&q).is_empty()); // drained
    }
}
