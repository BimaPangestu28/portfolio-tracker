//! Pure reconciliation: turn current app + Google state into a list of
//! operations. No DB or network here — the executor (Task 8) runs the ops.

use crate::google::calendar::{EventWrite, GCalEvent};
use crate::repo::events::EventRow;

/// Outbound operation derived from a pending-push local row.
pub enum OutboundOp {
    Create { event_id: i64, write: EventWrite },
    Patch { event_id: i64, google_event_id: String, etag: String, write: EventWrite },
    Delete { event_id: i64, google_event_id: String },
}

/// Inbound operation derived from one Google event.
pub enum InboundOp {
    UpsertForeign { google_event_id: String, etag: String, summary: String, location: Option<String>, notes: Option<String>, start_at: String },
    RemoveForeign { event_id: i64 },
    UpdateLocal { event_id: i64, etag: String, summary: String, location: Option<String>, notes: Option<String>, start_at: String },
    CancelLocal { event_id: i64 },
}

fn write_from_local(e: &EventRow) -> EventWrite {
    EventWrite {
        summary: e.title.clone(),
        location: e.location.clone(),
        description: e.notes.clone(),
        start_rfc3339_z: e.start_at.clone(),
    }
}

/// Map each pending-push local row to its outbound op.
pub fn plan_outbound(pending: &[EventRow]) -> Vec<OutboundOp> {
    pending.iter().filter_map(|e| {
        match (&e.google_event_id, e.status.as_str()) {
            (None, "cancelled") => None,
            (None, _) => Some(OutboundOp::Create { event_id: e.id, write: write_from_local(e) }),
            (Some(gid), "cancelled") => Some(OutboundOp::Delete { event_id: e.id, google_event_id: gid.clone() }),
            (Some(gid), _) => Some(OutboundOp::Patch {
                event_id: e.id,
                google_event_id: gid.clone(),
                etag: e.google_etag.clone().unwrap_or_default(),
                write: write_from_local(e),
            }),
        }
    }).collect()
}

/// Decide the inbound op for one Google event, given the matching app row (if any).
pub fn plan_inbound_one(r: &GCalEvent, existing: Option<&EventRow>) -> Option<InboundOp> {
    let start = r.start_rfc3339.clone().unwrap_or_default();
    let summary = r.summary.clone().unwrap_or_else(|| "(untitled)".into());
    match (existing, r.app_owned) {
        (None, false) => {
            if r.cancelled { None } else {
                Some(InboundOp::UpsertForeign {
                    google_event_id: r.id.clone(), etag: r.etag.clone(),
                    summary, location: r.location.clone(), notes: r.description.clone(), start_at: start,
                })
            }
        }
        (Some(row), false) => {
            if r.cancelled { Some(InboundOp::RemoveForeign { event_id: row.id }) }
            else {
                Some(InboundOp::UpsertForeign {
                    google_event_id: r.id.clone(), etag: r.etag.clone(),
                    summary, location: r.location.clone(), notes: r.description.clone(), start_at: start,
                })
            }
        }
        (Some(row), true) => {
            if r.cancelled { return Some(InboundOp::CancelLocal { event_id: row.id }); }
            if google_is_newer(row, r) {
                Some(InboundOp::UpdateLocal {
                    event_id: row.id, etag: r.etag.clone(),
                    summary, location: r.location.clone(), notes: r.description.clone(), start_at: start,
                })
            } else { None }
        }
        (None, true) => None,
    }
}

/// Google wins when its `updated` timestamp is strictly after our last sync.
fn google_is_newer(row: &EventRow, r: &GCalEvent) -> bool {
    let (Some(synced), Some(updated)) = (row.synced_at.as_deref(), r.updated.as_deref()) else {
        return true;
    };
    match (chrono::DateTime::parse_from_rfc3339(synced), chrono::DateTime::parse_from_rfc3339(updated)) {
        (Ok(s), Ok(u)) => u > s,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google::calendar::GCalEvent;
    use crate::repo::events::EventRow;

    fn local(id: i64, gid: Option<&str>, status: &str) -> EventRow {
        EventRow {
            id, title: "t".into(), location: None, notes: None,
            start_at: "2026-06-13T07:00:00Z".into(), status: status.into(),
            created_at: "2026-06-12T00:00:00+00:00".into(), source: "local".into(),
            google_event_id: gid.map(String::from), google_etag: gid.map(|_| "etag".into()),
            synced_at: gid.map(|_| "2026-06-12T00:00:00+00:00".into()),
            updated_at: Some("2026-06-12T00:00:00+00:00".into()),
        }
    }

    #[test]
    fn outbound_creates_unsynced_patches_synced_deletes_cancelled() {
        let pending = vec![
            local(1, None, "scheduled"),
            local(2, Some("g2"), "scheduled"),
            local(3, Some("g3"), "cancelled"),
        ];
        let ops = plan_outbound(&pending);
        assert!(matches!(ops[0], OutboundOp::Create { event_id: 1, .. }));
        assert!(matches!(ops[1], OutboundOp::Patch { event_id: 2, .. }));
        assert!(matches!(ops[2], OutboundOp::Delete { event_id: 3, .. }));
    }

    fn remote(id: &str, app_owned: bool, cancelled: bool, updated: &str) -> GCalEvent {
        GCalEvent {
            id: id.into(), etag: "e".into(), summary: Some("r".into()), location: None,
            description: None, start_rfc3339: Some("2026-06-13T07:00:00Z".into()),
            updated: Some(updated.into()), cancelled, app_owned,
        }
    }

    #[test]
    fn inbound_imports_foreign_as_readonly() {
        let r = remote("gf-1", false, false, "2026-06-12T09:00:00Z");
        let op = plan_inbound_one(&r, None);
        assert!(matches!(op, Some(InboundOp::UpsertForeign { .. })));
    }

    #[test]
    fn inbound_removes_deleted_foreign() {
        let existing = local(5, Some("gf-2"), "scheduled");
        let r = remote("gf-2", false, true, "2026-06-12T09:00:00Z");
        let op = plan_inbound_one(&r, Some(&existing));
        assert!(matches!(op, Some(InboundOp::RemoveForeign { event_id: 5 })));
    }

    #[test]
    fn inbound_app_owned_newer_in_google_updates_local() {
        let mut existing = local(7, Some("ga-1"), "scheduled");
        existing.synced_at = Some("2026-06-12T08:00:00+00:00".into());
        let r = remote("ga-1", true, false, "2026-06-12T09:00:00Z");
        let op = plan_inbound_one(&r, Some(&existing));
        assert!(matches!(op, Some(InboundOp::UpdateLocal { event_id: 7, .. })));
    }

    #[test]
    fn inbound_app_owned_deleted_in_google_cancels_local() {
        let existing = local(8, Some("ga-2"), "scheduled");
        let r = remote("ga-2", true, true, "2026-06-12T09:00:00Z");
        let op = plan_inbound_one(&r, Some(&existing));
        assert!(matches!(op, Some(InboundOp::CancelLocal { event_id: 8 })));
    }

    #[test]
    fn inbound_app_owned_not_newer_is_noop() {
        let mut existing = local(9, Some("ga-3"), "scheduled");
        existing.synced_at = Some("2026-06-12T10:00:00+00:00".into());
        let r = remote("ga-3", true, false, "2026-06-12T09:00:00Z");
        assert!(plan_inbound_one(&r, Some(&existing)).is_none());
    }
}
