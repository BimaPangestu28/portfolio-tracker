//! Issuer + payment details for invoices, from env (kept out of the repo —
//! public repo). `INVOICE_ISSUER_NAME` and `INVOICE_ACCOUNT_NO` are required;
//! the feature reports "belum dikonfigurasi" without them.

use crate::invoice::model::{Issuer, Payment};

pub struct InvoiceConfig {
    pub issuer: Issuer,
    pub payment: Payment,
    pub due_days: i64,
}

/// Read invoice config from env. Returns a human error (Indonesian) when the
/// required fields are missing, so the assistant can tell the owner.
pub fn from_env() -> Result<InvoiceConfig, String> {
    fn var(key: &str) -> String {
        std::env::var(key).unwrap_or_default()
    }
    let issuer_name = var("INVOICE_ISSUER_NAME");
    let account_no = var("INVOICE_ACCOUNT_NO");
    if issuer_name.trim().is_empty() || account_no.trim().is_empty() {
        return Err("invoice belum dikonfigurasi (set INVOICE_ISSUER_NAME, INVOICE_ACCOUNT_NO, dll di env)".into());
    }
    let due_days = var("INVOICE_DUE_DAYS").parse::<i64>().unwrap_or(14);
    Ok(InvoiceConfig {
        issuer: Issuer {
            name: issuer_name,
            company: var("INVOICE_ISSUER_COMPANY"),
            website: var("INVOICE_ISSUER_WEBSITE"),
            city: {
                let c = var("INVOICE_ISSUER_CITY");
                if c.is_empty() { "Jakarta".into() } else { c }
            },
        },
        payment: Payment {
            bank: var("INVOICE_BANK"),
            account_no,
            account_name: var("INVOICE_ACCOUNT_NAME"),
        },
        due_days,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_errors_without_issuer_name() {
        let prev_name = std::env::var("INVOICE_ISSUER_NAME").ok();
        let prev_acc = std::env::var("INVOICE_ACCOUNT_NO").ok();
        std::env::remove_var("INVOICE_ISSUER_NAME");
        std::env::remove_var("INVOICE_ACCOUNT_NO");
        let result = from_env();
        if let Some(v) = prev_name { std::env::set_var("INVOICE_ISSUER_NAME", v); }
        if let Some(v) = prev_acc { std::env::set_var("INVOICE_ACCOUNT_NO", v); }
        assert!(result.is_err());
    }
}
