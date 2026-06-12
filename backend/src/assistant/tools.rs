//! JSON-schema tool definitions for the assistant agent (Messages API format).

/// All assistant tools in Messages-API `tools` format.
pub fn definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "create_todo",
            "description": "Create a todo item for the user. Use when the user mentions a task they need to do.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Short task title in the user's words" },
                    "notes": { "type": "string", "description": "Optional extra detail" },
                    "due_at": { "type": "string", "description": "Optional deadline, RFC3339 with +07:00 offset, e.g. 2026-06-12T09:00:00+07:00" }
                },
                "required": ["title"]
            }
        },
        {
            "name": "list_todos",
            "description": "List the user's open todos with ids, titles, and due dates.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "complete_todo",
            "description": "Mark a todo as done. Look up the id with list_todos first if unsure.",
            "input_schema": {
                "type": "object",
                "properties": { "id": { "type": "integer", "description": "Todo id" } },
                "required": ["id"]
            }
        },
        {
            "name": "create_reminder",
            "description": "Schedule a reminder message to be sent to the user at a specific time, optionally recurring.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "message": { "type": "string", "description": "What to remind the user about" },
                    "remind_at": { "type": "string", "description": "When to fire, RFC3339 with +07:00 offset, must be in the future, e.g. 2026-06-12T09:00:00+07:00" },
                    "recurrence": { "type": "string", "enum": ["none", "daily", "weekly", "monthly"], "description": "Repeat pattern, default none" },
                    "todo_id": { "type": "integer", "description": "Optional todo this reminder belongs to (get the id from list_todos)" }
                },
                "required": ["message", "remind_at"]
            }
        },
        {
            "name": "list_reminders",
            "description": "List the user's pending reminders with ids, messages, and times.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "cancel_reminder",
            "description": "Cancel a pending reminder. Look up the id with list_reminders first if unsure.",
            "input_schema": {
                "type": "object",
                "properties": { "id": { "type": "integer", "description": "Reminder id" } },
                "required": ["id"]
            }
        },
        {
            "name": "get_portfolio_summary",
            "description": "Get the user's current investment portfolio snapshot: net worth, P&L, XIRR, allocation, holdings. Use for any finance/portfolio question.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "search_memory",
            "description": "Search the owner's long-term memory (facts learned from past conversations and notes). Use for recall questions like 'kapan aku bilang soal X?' or when past context would change the answer.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to look for, in natural language" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "remember",
            "description": "Save an explicit note to the owner's long-term memory. Use when the user asks you to remember something ('ingat ya ...', 'catat: ...' when it is a fact rather than a task).",
            "input_schema": {
                "type": "object",
                "properties": {
                    "note": { "type": "string", "description": "The fact to remember, as a standalone sentence" }
                },
                "required": ["note"]
            }
        },
        {
            "name": "create_event",
            "description": "Create an agenda event — something the owner attends or that has a time and place (meeting, appointment, dinner). Prefer this over create_reminder even when phrased as 'ingetin aku meeting besok jam 2'; use create_reminder only for standalone nudges with no agenda entry. A reminder fires 30 minutes before by default.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "What the event is" },
                    "start_at": { "type": "string", "description": "Start time, RFC3339 with +07:00 offset, must be in the future, e.g. 2026-06-13T14:00:00+07:00" },
                    "location": { "type": "string", "description": "Optional place" },
                    "notes": { "type": "string", "description": "Optional extra detail" },
                    "remind_minutes_before": { "type": "integer", "description": "Minutes before start to remind; default 30; 0 disables the reminder" }
                },
                "required": ["title", "start_at"]
            }
        },
        {
            "name": "list_events",
            "description": "List scheduled agenda events in a time range. Default: the next 7 days. Use for 'besok ada apa?', 'jadwal minggu ini'.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Range start, RFC3339 with +07:00; default now" },
                    "to": { "type": "string", "description": "Range end (exclusive), RFC3339 with +07:00; default from + 7 days" }
                }
            }
        },
        {
            "name": "cancel_event",
            "description": "Cancel a scheduled agenda event (its pre-event reminder is cancelled too). Look up the id with list_events first if unsure.",
            "input_schema": {
                "type": "object",
                "properties": { "id": { "type": "integer", "description": "Event id" } },
                "required": ["id"]
            }
        },
        {
            "name": "reject_review",
            "description": "Reject (discard) a pending ingest review item so it is not turned into a transaction. Get the id from list_pending_reviews.",
            "input_schema": {
                "type": "object",
                "properties": { "review_id": { "type": "integer", "description": "Review item id" } },
                "required": ["review_id"]
            }
        },
        {
            "name": "list_accounts",
            "description": "List the owner's investment accounts (id, name, type). Use before create_account to reuse an existing account, and to find an account_id for confirm_review.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "create_account",
            "description": "Create a new investment account (e.g. a broker like Nanovest the owner doesn't have yet). Always ask the user to confirm before calling — this writes data.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Account name, e.g. Nanovest" },
                    "account_type": { "type": "string", "description": "Type, e.g. broker, exchange, bank" },
                    "native_currency": { "type": "string", "description": "ISO currency code, e.g. IDR or USD" },
                    "institution": { "type": "string", "description": "Optional institution name" },
                    "note": { "type": "string", "description": "Optional note" }
                },
                "required": ["name", "account_type", "native_currency"]
            }
        },
        {
            "name": "list_pending_reviews",
            "description": "List ingest review items awaiting confirmation (from photos/PDFs the owner sent). Each line shows the review id, type, instrument, account, amounts, date, and flags items whose account or instrument isn't recognized yet. Use when the user asks to enter/confirm a transaction they sent.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "confirm_review",
            "description": "Confirm a pending review item, turning it into a transaction. Pass account_id and/or instrument_id to fill in anything flagged 'belum dikenali' (create the account first with create_account if needed). Always ask the user to confirm before calling — this writes a transaction.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "review_id": { "type": "integer", "description": "Review item id from list_pending_reviews" },
                    "account_id": { "type": "integer", "description": "Optional account id to set when the item's account is unknown" },
                    "instrument_id": { "type": "integer", "description": "Optional instrument id to set when the item's instrument is unknown" }
                },
                "required": ["review_id"]
            }
        },
        {
            "name": "list_instruments",
            "description": "List the owner's known instruments (id, symbol, name, type). Use to find an instrument_id for confirm_review when a review item's instrument shows 'belum dikenali' but the instrument may already exist under a slightly different name. If it genuinely doesn't exist, tell the user to add it in the web UI → Data (instruments can't be created from chat).",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "list_projects",
            "description": "List the owner's freelance projects (ClickUp lists). Use to find which project a task belongs to, and before create_project to avoid duplicates.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "create_project",
            "description": "Create a new freelance project (a ClickUp list) in the configured space. Always ask the user to confirm before calling — this creates data in ClickUp.",
            "input_schema": {
                "type": "object",
                "properties": { "name": { "type": "string", "description": "Project name, e.g. PT AIS" } },
                "required": ["name"]
            }
        }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defines_all_tools_with_schemas() {
        let defs = definitions();
        let names: Vec<&str> = defs
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "create_todo", "list_todos", "complete_todo",
                "create_reminder", "list_reminders", "cancel_reminder",
                "get_portfolio_summary", "search_memory", "remember",
                "create_event", "list_events", "cancel_event",
                "reject_review", "list_accounts", "create_account",
                "list_pending_reviews", "confirm_review", "list_instruments",
                "list_projects",
                "create_project",
            ]
        );
        for tool in defs.as_array().unwrap() {
            assert!(tool["description"].is_string(), "{} needs a description", tool["name"]);
            assert_eq!(tool["input_schema"]["type"], "object");
        }
    }

    #[test]
    fn required_fields_are_marked() {
        let defs = definitions();
        let find = |name: &str| {
            defs.as_array().unwrap().iter()
                .find(|t| t["name"] == name).unwrap().clone()
        };
        assert_eq!(find("create_todo")["input_schema"]["required"], serde_json::json!(["title"]));
        assert_eq!(
            find("create_reminder")["input_schema"]["required"],
            serde_json::json!(["message", "remind_at"])
        );
        assert_eq!(find("complete_todo")["input_schema"]["required"], serde_json::json!(["id"]));
        assert_eq!(find("cancel_reminder")["input_schema"]["required"], serde_json::json!(["id"]));
        assert_eq!(find("search_memory")["input_schema"]["required"], serde_json::json!(["query"]));
        assert_eq!(find("remember")["input_schema"]["required"], serde_json::json!(["note"]));
        assert_eq!(
            find("create_event")["input_schema"]["required"],
            serde_json::json!(["title", "start_at"])
        );
        assert_eq!(find("cancel_event")["input_schema"]["required"], serde_json::json!(["id"]));
        assert_eq!(find("reject_review")["input_schema"]["required"], serde_json::json!(["review_id"]));
        assert_eq!(find("confirm_review")["input_schema"]["required"], serde_json::json!(["review_id"]));
        assert_eq!(
            find("create_account")["input_schema"]["required"],
            serde_json::json!(["name", "account_type", "native_currency"])
        );
        assert_eq!(find("create_project")["input_schema"]["required"], serde_json::json!(["name"]));
    }
}
