//! Google Calendar integration: OAuth, a mockable Calendar client, and the
//! two-way sync reconciler. Separate from the financial `connectors` module.

pub mod calendar;
pub mod crypto;
pub mod engine;
pub mod gmail;
pub mod oauth;
pub mod sync;
