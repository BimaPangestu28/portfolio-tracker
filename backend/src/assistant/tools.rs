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
            "description": "Confirm a pending review item, turning it into a transaction. The account/instrument shown is only a guess read from the photo — pass account_id and/or instrument_id both to fill in anything flagged 'belum dikenali' AND to override a wrongly-detected account/instrument the owner corrects (e.g. the photo reads one broker but the holding is in another). You do NOT need the item to show 'belum dikenali' to override it (create the account first with create_account if needed). Always ask the user to confirm before calling — this writes a transaction.",
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
        },
        {
            "name": "create_task",
            "description": "Add a task to a freelance project (ClickUp). Pass the project name; if you don't know which project the user means and there is more than one, ask first. If the named project doesn't exist, offer to create it with create_project before retrying.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project (ClickUp list) name the task belongs to" },
                    "title": { "type": "string", "description": "What the task is" },
                    "due": { "type": "string", "description": "Optional due date, RFC3339 with +07:00 offset, e.g. 2026-06-14T17:00:00+07:00" },
                    "billable": { "type": "boolean", "description": "Mark the task billable (sets the ClickUp Billable field, if it exists)" },
                    "amount": { "type": "number", "description": "Billable amount in IDR (sets the ClickUp Amount field, if it exists)" }
                },
                "required": ["project", "title"]
            }
        },
        {
            "name": "list_tasks",
            "description": "List freelance tasks from ClickUp. Pass a project name to list that project's open tasks; or pass scope 'today' / 'overdue' to list due/overdue tasks across all projects. Default scope 'open' lists all open tasks.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Optional project (list) name to filter to" },
                    "scope": { "type": "string", "enum": ["open", "today", "overdue"], "description": "open (default), today (due today), or overdue" }
                }
            }
        },
        {
            "name": "complete_task",
            "description": "Mark a freelance task complete in ClickUp. Get the task_id from list_tasks (shown in brackets).",
            "input_schema": {
                "type": "object",
                "properties": { "task_id": { "type": "string", "description": "ClickUp task id from list_tasks" } },
                "required": ["task_id"]
            }
        },
        {
            "name": "draft_proposal",
            "description": "Draft an Upwork job proposal in professional English from a job description the user pastes. Pulls the owner's skills and experience from long-term memory to tailor it. Returns the draft for the user to review and submit manually — it never submits anything. Use when the owner pastes a job and asks for a proposal.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "job_text": { "type": "string", "description": "The full Upwork job description the user pasted." },
                    "notes": { "type": "string", "description": "Optional emphasis or constraints, e.g. 'emphasize React, bid $30/hr'." }
                },
                "required": ["job_text"]
            }
        },
        {
            "name": "list_clients",
            "description": "List saved invoice clients (name). Use to reuse an existing client before create_invoice.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "create_invoice",
            "description": "Create and send a freelance invoice PDF over Telegram. Dictate the client and line items. If the client is new, first ask for their details (sub_name/website) and pass client_details. Echo the parsed items + total to the user before calling so they can catch typos.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "client_name": { "type": "string", "description": "Client name, e.g. PT AIS" },
                    "line_items": {
                        "type": "array",
                        "description": "Invoice lines",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string", "description": "Bold line, e.g. Pengembangan Landing Page" },
                                "body": { "type": "string", "description": "Optional description paragraph" },
                                "qty": { "type": "integer", "description": "Quantity, default 1" },
                                "amount": { "type": "integer", "description": "Line total in IDR (e.g. 10 juta -> 10000000)" }
                            },
                            "required": ["title", "amount"]
                        }
                    },
                    "client_details": {
                        "type": "object",
                        "description": "Only when the client is new",
                        "properties": { "sub_name": { "type": "string" }, "website": { "type": "string" } }
                    },
                    "due_days": { "type": "integer", "description": "Override default due-in days" }
                },
                "required": ["client_name", "line_items"]
            }
        },
        {
            "name": "capture_to_inbox",
            "description": "Save a raw quick capture to the GTD inbox for later sorting. Use for vague/ambiguous dumps with no clear single action (e.g. 'inget beliin kado', 'ide fitur X').",
            "input_schema": {
                "type": "object",
                "properties": { "content": { "type": "string", "description": "The raw captured text" } },
                "required": ["content"]
            }
        },
        {
            "name": "list_inbox",
            "description": "List pending inbox captures with ids. Use for 'apa di inbox?' or before sorting.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "resolve_inbox",
            "description": "Mark inbox items as sorted (after you created todos/events/tasks/notes from them) or dropped (discarded). Pass the item ids.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "ids": { "type": "array", "items": { "type": "integer" }, "description": "Inbox item ids to resolve" },
                    "status": { "type": "string", "enum": ["sorted", "dropped"], "description": "sorted (handled) or dropped (discarded)" }
                },
                "required": ["ids", "status"]
            }
        },
        {
            "name": "list_emails",
            "description": "List important unread Gmail (sender, subject, snippet, id). Use for 'ada email penting?'.",
            "input_schema": { "type": "object", "properties": { "max": { "type": "integer", "description": "Max emails, default 10" } } }
        },
        {
            "name": "read_email",
            "description": "Read the full body of one email by id (from list_emails) to summarize it or to draft a reply.",
            "input_schema": { "type": "object", "properties": { "id": { "type": "string", "description": "Gmail message id" } }, "required": ["id"] }
        },
        {
            "name": "draft_reply",
            "description": "Create a Gmail DRAFT reply to an email (by id). You compose the body; the draft is saved in Gmail for the owner to review and send — it is NOT sent automatically.",
            "input_schema": { "type": "object", "properties": { "id": { "type": "string", "description": "Gmail message id to reply to" }, "body": { "type": "string", "description": "Reply text you composed" } }, "required": ["id", "body"] }
        },
        {
            "name": "cashflow_summary",
            "description": "Monthly cashflow: money in, money out, net, top expense categories, and freelance invoiced that month. Use for 'bulan ini masuk/kepake/net berapa?'.",
            "input_schema": { "type": "object", "properties": { "month": { "type": "string", "description": "YYYY-MM; default current month" } } }
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
                "create_task",
                "list_tasks",
                "complete_task",
                "draft_proposal",
                "list_clients",
                "create_invoice",
                "capture_to_inbox",
                "list_inbox",
                "resolve_inbox",
                "list_emails",
                "read_email",
                "draft_reply",
                "cashflow_summary",
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
        assert_eq!(find("create_task")["input_schema"]["required"], serde_json::json!(["project", "title"]));
        assert_eq!(find("complete_task")["input_schema"]["required"], serde_json::json!(["task_id"]));
        assert_eq!(find("draft_proposal")["input_schema"]["required"], serde_json::json!(["job_text"]));
        assert_eq!(find("create_invoice")["input_schema"]["required"], serde_json::json!(["client_name", "line_items"]));
    }

    /// The detected account/instrument is only an OCR guess; the assistant must
    /// know confirm_review can OVERRIDE it, not merely fill in 'belum dikenali'.
    #[test]
    fn confirm_review_description_allows_overriding_a_detected_account() {
        let defs = definitions();
        let desc = defs
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "confirm_review")
            .unwrap()["description"]
            .as_str()
            .unwrap()
            .to_lowercase();
        assert!(
            desc.contains("override"),
            "confirm_review must say account/instrument can be overridden/corrected, \
             not only filled when 'belum dikenali': {desc}"
        );
    }
}
