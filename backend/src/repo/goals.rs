use crate::db::Db;
use crate::repo::dec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct GoalRow {
    pub id: i64,
    pub label: String,
    pub note: Option<String>,
    pub target_idr: String,
    pub current_kind: String,
    pub current_manual_idr: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub target_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewGoal {
    pub label: String,
    pub note: Option<String>,
    pub target_idr: String,
    pub current_kind: String,
    pub current_manual_idr: Option<String>,
    pub sort_order: Option<i64>,
    pub target_date: Option<String>,
}

const VALID_KINDS: &[&str] = &["cash", "networth", "manual", "tagged"];

pub async fn create(db: &Db, g: &NewGoal) -> anyhow::Result<GoalRow> {
    if !VALID_KINDS.contains(&g.current_kind.as_str()) {
        anyhow::bail!(
            "current_kind must be one of {:?}, got '{}'",
            VALID_KINDS,
            g.current_kind
        );
    }
    // Validate target is a valid decimal
    dec(&g.target_idr)?;

    // If kind=manual, require and validate current_manual_idr
    if g.current_kind == "manual" {
        match g.current_manual_idr.as_deref() {
            Some(v) => { dec(v)?; }
            None => anyhow::bail!("current_manual_idr is required when current_kind='manual'"),
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    let sort_order = g.sort_order.unwrap_or(0);

    let id = sqlx::query(
        "INSERT INTO goal (label, note, target_idr, current_kind, current_manual_idr, sort_order, created_at, target_date)
         VALUES (?,?,?,?,?,?,?,?)",
    )
    .bind(&g.label)
    .bind(&g.note)
    .bind(&g.target_idr)
    .bind(&g.current_kind)
    .bind(&g.current_manual_idr)
    .bind(sort_order)
    .bind(&now)
    .bind(&g.target_date)
    .execute(db)
    .await?
    .last_insert_rowid();

    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<GoalRow> {
    Ok(
        sqlx::query_as::<_, GoalRow>("SELECT * FROM goal WHERE id = ?")
            .bind(id)
            .fetch_one(db)
            .await?,
    )
}

pub async fn list(db: &Db) -> anyhow::Result<Vec<GoalRow>> {
    Ok(
        sqlx::query_as::<_, GoalRow>("SELECT * FROM goal ORDER BY sort_order, id")
            .fetch_all(db)
            .await?,
    )
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    // txn.goal_id REFERENCES goal(id) with foreign_keys=ON would reject the
    // delete while any txn is still tagged. Untag first, then delete, atomically.
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE txn SET goal_id = NULL WHERE goal_id = ?")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM goal WHERE id = ?")
        .bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Partial update; absent fields keep their current values.
#[derive(Debug, Deserialize)]
pub struct UpdateGoal {
    pub label: Option<String>,
    pub note: Option<String>,
    pub target_idr: Option<String>,
    pub current_kind: Option<String>,
    pub current_manual_idr: Option<String>,
    pub target_date: Option<String>,
    pub sort_order: Option<i64>,
}

pub async fn update(db: &Db, id: i64, u: &UpdateGoal) -> anyhow::Result<GoalRow> {
    let cur = get(db, id).await?;
    let current_kind = u.current_kind.clone().unwrap_or(cur.current_kind);
    if !VALID_KINDS.contains(&current_kind.as_str()) {
        anyhow::bail!("current_kind must be one of {VALID_KINDS:?}, got '{current_kind}'");
    }
    let target_idr = u.target_idr.clone().unwrap_or(cur.target_idr);
    dec(&target_idr)?;
    let label = u.label.clone().unwrap_or(cur.label);
    let note = u.note.clone().or(cur.note);
    let current_manual_idr = u.current_manual_idr.clone().or(cur.current_manual_idr);
    if current_kind == "manual" {
        match current_manual_idr.as_deref() {
            Some(v) => { dec(v)?; }
            None => anyhow::bail!("current_manual_idr is required when current_kind='manual'"),
        }
    }
    let target_date = u.target_date.clone().or(cur.target_date);
    let sort_order = u.sort_order.unwrap_or(cur.sort_order);

    sqlx::query(
        "UPDATE goal SET label=?, note=?, target_idr=?, current_kind=?, current_manual_idr=?, target_date=?, sort_order=? WHERE id=?",
    )
    .bind(&label).bind(&note).bind(&target_idr).bind(&current_kind)
    .bind(&current_manual_idr).bind(&target_date).bind(sort_order).bind(id)
    .execute(db).await?;
    get(db, id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn create_and_list() {
        let db = mem_db().await;
        let g = NewGoal {
            label: "Dana Darurat".into(),
            note: Some("6x pengeluaran".into()),
            target_idr: "136000000".into(),
            current_kind: "cash".into(),
            current_manual_idr: None,
            sort_order: Some(1),
            target_date: None,
        };
        let created = create(&db, &g).await.unwrap();
        assert_eq!(created.label, "Dana Darurat");
        assert_eq!(created.current_kind, "cash");
        assert_eq!(created.target_idr, "136000000");
        assert!(created.note.is_some());

        let all = list(&db).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, created.id);
    }

    #[tokio::test]
    async fn create_networth_kind() {
        let db = mem_db().await;
        let g = NewGoal {
            label: "FIRE".into(),
            note: None,
            target_idr: "6792000000".into(),
            current_kind: "networth".into(),
            current_manual_idr: None,
            sort_order: None,
            target_date: None,
        };
        let created = create(&db, &g).await.unwrap();
        assert_eq!(created.current_kind, "networth");
    }

    #[tokio::test]
    async fn create_manual_kind_requires_current_manual_idr() {
        let db = mem_db().await;
        // Should fail without current_manual_idr
        let g_no_manual = NewGoal {
            label: "DP Rumah".into(),
            note: None,
            target_idr: "500000000".into(),
            current_kind: "manual".into(),
            current_manual_idr: None,
            sort_order: None,
            target_date: None,
        };
        assert!(create(&db, &g_no_manual).await.is_err());

        // Should succeed with current_manual_idr
        let g_with_manual = NewGoal {
            label: "DP Rumah".into(),
            note: None,
            target_idr: "500000000".into(),
            current_kind: "manual".into(),
            current_manual_idr: Some("182160000".into()),
            sort_order: None,
            target_date: None,
        };
        let created = create(&db, &g_with_manual).await.unwrap();
        assert_eq!(created.current_manual_idr.as_deref(), Some("182160000"));
    }

    #[tokio::test]
    async fn reject_bad_kind() {
        let db = mem_db().await;
        let g = NewGoal {
            label: "Bad".into(),
            note: None,
            target_idr: "1000000".into(),
            current_kind: "invalid_kind".into(),
            current_manual_idr: None,
            sort_order: None,
            target_date: None,
        };
        assert!(create(&db, &g).await.is_err());
        assert_eq!(list(&db).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn reject_bad_decimal_target() {
        let db = mem_db().await;
        let g = NewGoal {
            label: "Bad Dec".into(),
            note: None,
            target_idr: "not_a_number".into(),
            current_kind: "cash".into(),
            current_manual_idr: None,
            sort_order: None,
            target_date: None,
        };
        assert!(create(&db, &g).await.is_err());
        assert_eq!(list(&db).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn delete_goal() {
        let db = mem_db().await;
        let g = NewGoal {
            label: "To Delete".into(),
            note: None,
            target_idr: "100000".into(),
            current_kind: "cash".into(),
            current_manual_idr: None,
            sort_order: None,
            target_date: None,
        };
        let created = create(&db, &g).await.unwrap();
        delete(&db, created.id).await.unwrap();
        assert_eq!(list(&db).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn create_tagged_goal_with_target_date() {
        let db = mem_db().await;
        let g = NewGoal {
            label: "Pendidikan Anak".into(), note: Some("SD 2035".into()),
            target_idr: "200000000".into(), current_kind: "tagged".into(),
            current_manual_idr: None, sort_order: None, target_date: Some("2035-06-01".into()),
        };
        let created = create(&db, &g).await.unwrap();
        assert_eq!(created.current_kind, "tagged");
        assert_eq!(created.target_date.as_deref(), Some("2035-06-01"));
    }

    #[tokio::test]
    async fn update_changes_fields_keeping_others() {
        let db = mem_db().await;
        let created = create(&db, &NewGoal {
            label: "Dana Darurat".into(), note: None, target_idr: "100000000".into(),
            current_kind: "cash".into(), current_manual_idr: None, sort_order: Some(1), target_date: None,
        }).await.unwrap();

        let updated = update(&db, created.id, &UpdateGoal {
            target_idr: Some("136000000".into()),
            target_date: Some("2027-01-01".into()),
            label: None, note: None, current_kind: None, current_manual_idr: None, sort_order: None,
        }).await.unwrap();

        assert_eq!(updated.target_idr, "136000000");
        assert_eq!(updated.target_date.as_deref(), Some("2027-01-01"));
        assert_eq!(updated.label, "Dana Darurat"); // preserved
        assert_eq!(updated.current_kind, "cash");   // preserved
    }

    #[tokio::test]
    async fn update_rejects_bad_kind() {
        let db = mem_db().await;
        let created = create(&db, &NewGoal {
            label: "X".into(), note: None, target_idr: "1".into(),
            current_kind: "cash".into(), current_manual_idr: None, sort_order: None, target_date: None,
        }).await.unwrap();
        assert!(update(&db, created.id, &UpdateGoal {
            current_kind: Some("bogus".into()),
            label: None, note: None, target_idr: None, current_manual_idr: None, target_date: None, sort_order: None,
        }).await.is_err());
    }

    #[tokio::test]
    async fn delete_goal_untags_its_transactions() {
        let db = mem_db().await;
        let acc = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let ins = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument { symbol:"BBCA".into(), name:"BCA".into(), instrument_type:"stock_id".into(), native_currency:"IDR".into(), category_id:None, price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
        let goal = create(&db, &NewGoal { label:"G".into(), note:None, target_idr:"1".into(), current_kind:"tagged".into(), current_manual_idr:None, sort_order:None, target_date:None }).await.unwrap();
        let t = crate::repo::transactions::create(&db, &crate::repo::transactions::NewTransaction { account_id:acc.id, instrument_id:ins.id, txn_type:"buy".into(), executed_at:chrono::Utc::now(), quantity:"1".into(), price_native:"1".into(), fee_native:None, currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
        crate::repo::transactions::set_txn_goal(&db, t.id, Some(goal.id)).await.unwrap();

        // Must NOT fail with an FK violation.
        delete(&db, goal.id).await.unwrap();

        assert!(get(&db, goal.id).await.is_err());                       // goal gone
        assert!(crate::repo::transactions::list_by_goal(&db, goal.id).await.unwrap().is_empty()); // txn untagged
    }
}
