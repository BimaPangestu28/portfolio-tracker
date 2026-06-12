//! ClickUp REST integration: a thin client behind a trait seam so the
//! assistant's project/task tools can be tested with a fake.
pub mod client;

pub use client::{ClickUpApi, ClickUpClient, ClickUpError, NewTask, Project};
