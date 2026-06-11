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
    }
}
