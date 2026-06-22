use crate::db::Db;
use crate::repo::dec;
use serde::{Deserialize, Serialize};

const VALID_BIND_KINDS: &[&str] = &["group", "category", "instrument"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlanNodeRow {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub target_pct: String,
    pub tolerance_band_pct: Option<String>,
    pub bind_kind: String,
    pub category_id: Option<i64>,
    pub instrument_id: Option<i64>,
    pub sort_order: i64,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewPlanNode {
    pub parent_id: Option<i64>,
    pub name: String,
    pub target_pct: String,
    pub tolerance_band_pct: Option<String>,
    pub bind_kind: String,
    pub category_id: Option<i64>,
    pub instrument_id: Option<i64>,
    pub sort_order: Option<i64>,
    pub color: Option<String>,
}

/// Partial update; absent fields keep their current values. Binding fields
/// (bind_kind/category_id/instrument_id) are intentionally immutable — change a
/// node's binding by deleting and recreating it.
#[derive(Debug, Deserialize)]
pub struct UpdatePlanNode {
    pub name: Option<String>,
    pub target_pct: Option<String>,
    pub tolerance_band_pct: Option<String>,
    pub sort_order: Option<i64>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MovePlanNode {
    pub parent_id: Option<i64>,
    pub sort_order: i64,
}

/// Enforce the bind_kind invariants:
/// - 'instrument' => instrument_id set, category_id null
/// - 'category'   => category_id set, instrument_id null
/// - 'group'      => both null
fn validate_bind(bind_kind: &str, category_id: Option<i64>, instrument_id: Option<i64>) -> anyhow::Result<()> {
    if !VALID_BIND_KINDS.contains(&bind_kind) {
        anyhow::bail!("bind_kind must be one of {VALID_BIND_KINDS:?}, got '{bind_kind}'");
    }
    match bind_kind {
        "instrument" if instrument_id.is_none() || category_id.is_some() =>
            anyhow::bail!("bind_kind='instrument' requires instrument_id and no category_id"),
        "category" if category_id.is_none() || instrument_id.is_some() =>
            anyhow::bail!("bind_kind='category' requires category_id and no instrument_id"),
        "group" if category_id.is_some() || instrument_id.is_some() =>
            anyhow::bail!("bind_kind='group' must not set category_id or instrument_id"),
        _ => Ok(()),
    }
}

pub async fn create(db: &Db, n: &NewPlanNode) -> anyhow::Result<PlanNodeRow> {
    validate_bind(&n.bind_kind, n.category_id, n.instrument_id)?;
    dec(&n.target_pct)?;
    if let Some(t) = n.tolerance_band_pct.as_deref() { dec(t)?; }
    let id = sqlx::query(
        "INSERT INTO plan_node (parent_id, name, target_pct, tolerance_band_pct, bind_kind, category_id, instrument_id, sort_order, color)
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind(n.parent_id).bind(&n.name).bind(&n.target_pct).bind(&n.tolerance_band_pct)
    .bind(&n.bind_kind).bind(n.category_id).bind(n.instrument_id)
    .bind(n.sort_order.unwrap_or(0)).bind(&n.color)
    .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<PlanNodeRow> {
    Ok(sqlx::query_as::<_, PlanNodeRow>("SELECT * FROM plan_node WHERE id = ?")
        .bind(id).fetch_one(db).await?)
}

pub async fn list(db: &Db) -> anyhow::Result<Vec<PlanNodeRow>> {
    Ok(sqlx::query_as::<_, PlanNodeRow>("SELECT * FROM plan_node ORDER BY sort_order, id")
        .fetch_all(db).await?)
}

pub async fn update(db: &Db, id: i64, u: &UpdatePlanNode) -> anyhow::Result<PlanNodeRow> {
    let cur = get(db, id).await?;
    if let Some(t) = u.target_pct.as_deref() { dec(t)?; }
    if let Some(t) = u.tolerance_band_pct.as_deref() { dec(t)?; }
    let name = u.name.clone().unwrap_or(cur.name);
    let target_pct = u.target_pct.clone().unwrap_or(cur.target_pct);
    let tolerance = u.tolerance_band_pct.clone().or(cur.tolerance_band_pct);
    let sort_order = u.sort_order.unwrap_or(cur.sort_order);
    let color = u.color.clone().or(cur.color);
    sqlx::query("UPDATE plan_node SET name=?, target_pct=?, tolerance_band_pct=?, sort_order=?, color=? WHERE id=?")
        .bind(&name).bind(&target_pct).bind(&tolerance).bind(sort_order).bind(&color).bind(id)
        .execute(db).await?;
    get(db, id).await
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    // ON DELETE CASCADE (foreign_keys pragma is ON) removes the subtree.
    sqlx::query("DELETE FROM plan_node WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

/// Reparent and/or reorder a node. Rejects a move that would put the node under
/// one of its own descendants (cycle), which SQLite would otherwise allow.
pub async fn move_node(db: &Db, id: i64, m: &MovePlanNode) -> anyhow::Result<PlanNodeRow> {
    get(db, id).await?; // 404 surfaces as RowNotFound -> caller maps to NotFound
    if let Some(new_parent) = m.parent_id {
        if new_parent == id {
            anyhow::bail!("a node cannot be its own parent");
        }
        // Walk up from the proposed parent; if we reach `id`, it's a cycle.
        let rows = list(db).await?;
        let parent_of: std::collections::HashMap<i64, Option<i64>> =
            rows.iter().map(|r| (r.id, r.parent_id)).collect();
        let mut cur = Some(new_parent);
        while let Some(c) = cur {
            if c == id {
                anyhow::bail!("move would create a cycle");
            }
            cur = parent_of.get(&c).copied().flatten();
        }
    }
    sqlx::query("UPDATE plan_node SET parent_id=?, sort_order=? WHERE id=?")
        .bind(m.parent_id).bind(m.sort_order).bind(id)
        .execute(db).await?;
    get(db, id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> crate::db::Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn create_and_list_round_trip() {
        // NB: the migration backfill runs at connect() time against whatever
        // categories exist then. A fresh in-memory test DB has none, so backfill
        // produces no rows here — backfill is verified by the migration booting
        // cleanly (db:: tests) and by prod data, not by this unit test. Here we
        // create a category first so the category_id FK is satisfied (foreign_keys=ON).
        let db = mem_db().await;
        let cat = crate::repo::categories::create(&db, &crate::repo::categories::NewCategory {
            name: "Saham IDX".into(), target_pct: "30".into(),
            tolerance_band_pct: Some("5".into()), sort_order: Some(1), color: None,
        }).await.unwrap();
        let made = create(&db, &NewPlanNode {
            parent_id: None, name: "Saham".into(), target_pct: "30".into(),
            tolerance_band_pct: Some("5".into()), bind_kind: "category".into(),
            category_id: Some(cat.id), instrument_id: None, sort_order: Some(0), color: None,
        }).await.unwrap();
        assert_eq!(made.bind_kind, "category");
        assert_eq!(made.category_id, Some(cat.id));
        let all = list(&db).await.unwrap();
        assert!(all.iter().any(|n| n.id == made.id));
    }

    #[tokio::test]
    async fn rejects_instrument_bind_without_instrument_id() {
        let db = mem_db().await;
        let r = create(&db, &NewPlanNode {
            parent_id: None, name: "Bad".into(), target_pct: "10".into(),
            tolerance_band_pct: None, bind_kind: "instrument".into(),
            category_id: None, instrument_id: None, sort_order: None, color: None,
        }).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn rejects_group_bind_with_a_binding() {
        let db = mem_db().await;
        let r = create(&db, &NewPlanNode {
            parent_id: None, name: "Bad".into(), target_pct: "10".into(),
            tolerance_band_pct: None, bind_kind: "group".into(),
            category_id: Some(1), instrument_id: None, sort_order: None, color: None,
        }).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn update_changes_target_keeping_other_fields() {
        let db = mem_db().await;
        let n = create(&db, &NewPlanNode {
            parent_id: None, name: "Saham".into(), target_pct: "30".into(),
            tolerance_band_pct: Some("5".into()), bind_kind: "group".into(),
            category_id: None, instrument_id: None, sort_order: None, color: None,
        }).await.unwrap();
        let u = update(&db, n.id, &UpdatePlanNode {
            name: None, target_pct: Some("40".into()),
            tolerance_band_pct: None, sort_order: None, color: None,
        }).await.unwrap();
        assert_eq!(u.target_pct, "40");
        assert_eq!(u.name, "Saham");
        assert_eq!(u.tolerance_band_pct.as_deref(), Some("5"));
    }

    #[tokio::test]
    async fn delete_cascades_to_children() {
        let db = mem_db().await;
        let root = create(&db, &NewPlanNode {
            parent_id: None, name: "Saham".into(), target_pct: "30".into(),
            tolerance_band_pct: None, bind_kind: "group".into(),
            category_id: None, instrument_id: None, sort_order: None, color: None,
        }).await.unwrap();
        let child = create(&db, &NewPlanNode {
            parent_id: Some(root.id), name: "BBCA".into(), target_pct: "40".into(),
            tolerance_band_pct: None, bind_kind: "group".into(),
            category_id: None, instrument_id: None, sort_order: None, color: None,
        }).await.unwrap();
        delete(&db, root.id).await.unwrap();
        let all = list(&db).await.unwrap();
        assert!(!all.iter().any(|n| n.id == root.id || n.id == child.id));
    }

    #[tokio::test]
    async fn move_rejects_cycle() {
        let db = mem_db().await;
        let a = create(&db, &NewPlanNode {
            parent_id: None, name: "A".into(), target_pct: "50".into(),
            tolerance_band_pct: None, bind_kind: "group".into(),
            category_id: None, instrument_id: None, sort_order: None, color: None,
        }).await.unwrap();
        let b = create(&db, &NewPlanNode {
            parent_id: Some(a.id), name: "B".into(), target_pct: "50".into(),
            tolerance_band_pct: None, bind_kind: "group".into(),
            category_id: None, instrument_id: None, sort_order: None, color: None,
        }).await.unwrap();
        // Making A a child of B would create a cycle (B is A's descendant).
        let r = move_node(&db, a.id, &MovePlanNode { parent_id: Some(b.id), sort_order: 0 }).await;
        assert!(r.is_err());
    }
}
