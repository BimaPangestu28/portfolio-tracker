//! Read-only customer-service tool specifications (Anthropic tool-use shape).
//! This is the ENTIRE surface the CS agent can act through — deliberately narrow,
//! and with no access to any owner/Noah tool.

/// The CS tool registry. Returns a JSON array in the Anthropic tool-use shape.
pub fn definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "kb_search",
            "description": "Search the business knowledge base (FAQ, docs, policies) for information to answer the customer. ALWAYS use this before answering a factual question; never invent facts.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to look up, in the customer's words" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "get_pricing",
            "description": "List the currently available products/packages with prices and availability. Use when the customer asks about price, packages, or what is offered.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Optional filter on what they're interested in" }
                },
                "required": []
            }
        },
        {
            "name": "lookup_order",
            "description": "Look up the status of an order/booking. Requires BOTH the order reference AND the email or phone the customer used — for their privacy you cannot look up an order without a matching contact.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "order_ref": { "type": "string", "description": "The order/booking reference the customer quotes" },
                    "contact":   { "type": "string", "description": "The email or phone on the order, to verify ownership" }
                },
                "required": ["order_ref", "contact"]
            }
        },
        {
            "name": "escalate_to_human",
            "description": "Hand this conversation to a human agent. Use when you cannot answer from the knowledge base/tools, when the customer explicitly asks for a human, or for sensitive/complaint situations. The customer will be told a human will follow up.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "reason":  { "type": "string", "enum": ["cannot_answer", "customer_request", "sensitive"], "description": "Why you are escalating" },
                    "summary": { "type": "string", "description": "One-paragraph summary of what the customer needs, for the human" }
                },
                "required": ["reason", "summary"]
            }
        }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_expose_exactly_the_four_cs_tools() {
        let defs  = definitions();
        let names: Vec<&str> = defs
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["kb_search", "get_pricing", "lookup_order", "escalate_to_human"]);
    }

    #[test]
    fn lookup_order_requires_ref_and_contact() {
        let defs   = definitions();
        let lookup = defs
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "lookup_order")
            .unwrap();
        let required = lookup["input_schema"]["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r == "order_ref"));
        assert!(required.iter().any(|r| r == "contact"));
    }
}
