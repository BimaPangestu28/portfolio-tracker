//! Display-ready invoice data. The handler formats money (e.g. "Rp 12.000.000")
//! and computes `terbilang` before building this; rendering just lays it out.

#[derive(Debug, Clone)]
pub struct Issuer {
    pub name: String,
    pub company: String,
    pub website: String,
    pub city: String,
}

#[derive(Debug, Clone)]
pub struct Payment {
    pub bank: String,
    pub account_no: String,
    pub account_name: String,
}

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub name: String,
    pub sub_name: Option<String>,
    pub website: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LineItem {
    pub title: String,
    pub body: Option<String>,
    pub qty: String,
    pub unit_price: String,
    pub amount: String,
}

#[derive(Debug, Clone)]
pub struct InvoiceData {
    pub number: String,
    pub issue_date: String,
    pub due_date: String,
    pub issuer: Issuer,
    pub client: ClientInfo,
    pub payment: Payment,
    pub line_items: Vec<LineItem>,
    pub subtotal: String,
    pub total: String,
    pub terbilang: String,
}
