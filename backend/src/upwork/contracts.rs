//! Upwork contracts → ClickUp project (List) sync. Pure helpers here; the
//! `run_pass` orchestration and polling loop are added in later tasks. One-way,
//! create-only; idempotent via the `upwork_project_link` mapping.

use crate::upwork::client::Contract;

/// The ClickUp List name for a contract: "{client} — {title}", or just the
/// title when the client name is empty.
pub fn list_name(contract: &Contract) -> String {
    if contract.client_name.trim().is_empty() {
        contract.title.clone()
    } else {
        format!("{} — {}", contract.client_name, contract.title)
    }
}

/// Plain-text Telegram alert announcing a synced contract (no Markdown).
pub fn format_created_alert(name: &str) -> String {
    format!("🗂 New Upwork contract synced to ClickUp: {name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(client: &str, title: &str) -> Contract {
        Contract {
            id: "c1".into(), title: title.into(), client_name: client.into(), status: "active".into(),
        }
    }

    #[test]
    fn list_name_joins_client_and_title() {
        assert_eq!(list_name(&contract("Acme", "Build API")), "Acme — Build API");
    }

    #[test]
    fn list_name_falls_back_to_title_when_client_empty() {
        assert_eq!(list_name(&contract("   ", "Build API")), "Build API");
    }

    #[test]
    fn created_alert_has_name_no_markdown() {
        let msg = format_created_alert("Acme — Build API");
        assert!(msg.contains("Acme — Build API"));
        assert!(!msg.contains("**"));
    }
}
