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
}

#[derive(Debug, Deserialize)]
pub struct NewGoal {
    pub label: String,
    pub note: Option<String>,
    pub target_idr: String,
    pub current_kind: String,
    pub current_manual_idr: Option<String>,
    pub sort_order: Option<i64>,
}

const VALID_KINDS: &[&str] = &["cash", "networth", "manual"];

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
        "INSERT INTO goal (label, note, target_idr, current_kind, current_manual_idr, sort_order, created_at)
         VALUES (?,?,?,?,?,?,?)",
    )
    .bind(&g.label)
    .bind(&g.note)
    .bind(&g.target_idr)
    .bind(&g.current_kind)
    .bind(&g.current_manual_idr)
    .bind(sort_order)
    .bind(&now)
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
    sqlx::query("DELETE FROM goal WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
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
        };
        let created = create(&db, &g).await.unwrap();
        delete(&db, created.id).await.unwrap();
        assert_eq!(list(&db).await.unwrap().len(), 0);
    }
}
