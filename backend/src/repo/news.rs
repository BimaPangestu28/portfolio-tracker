//! Persistence for the daily news digest (migration 0022).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ArticleRow {
    pub position: i64,
    pub title: String,
    pub url: String,
    pub source: String,
    pub score: i64,
    pub summary: String,
    /// JSON array of strings.
    pub key_points: String,
    pub image_url: Option<String>,
    pub read_minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct QuizRow {
    pub position: i64,
    pub article_pos: Option<i64>,
    pub question: String,
    /// JSON array of strings.
    pub options: String,
    pub answer_index: i64,
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DateRow {
    pub digest_date: String,
    pub created_at: String,
    pub article_count: i64,
}

pub struct NewArticle {
    pub position: i64,
    pub title: String,
    pub url: String,
    pub source: String,
    pub score: i64,
    pub summary: String,
    pub key_points_json: String,
    pub image_url: Option<String>,
    pub read_minutes: Option<i64>,
}

pub struct NewQuiz {
    pub position: i64,
    pub article_pos: Option<i64>,
    pub question: String,
    pub options_json: String,
    pub answer_index: i64,
    pub explanation: Option<String>,
}

/// True if a digest already exists for the given WIB date.
pub async fn exists(db: &Db, date: &str) -> anyhow::Result<bool> {
    let row: Option<(String,)> = sqlx::query_as("SELECT digest_date FROM news_digest WHERE digest_date = ?")
        .bind(date)
        .fetch_optional(db)
        .await?;
    Ok(row.is_some())
}

/// Insert a full digest (header + articles + quiz) in one transaction.
pub async fn insert(
    db: &Db,
    date: &str,
    created_at: &str,
    articles: &[NewArticle],
    quiz: &[NewQuiz],
) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;
    // Plain INSERT (not OR IGNORE): `ensure_today` guarantees this runs exactly
    // once per date via the proactive_log claim, so a duplicate here is a real
    // bug we want to surface (PK violation → rollback) rather than swallow.
    sqlx::query("INSERT INTO news_digest (digest_date, created_at) VALUES (?, ?)")
        .bind(date).bind(created_at).execute(&mut *tx).await?;
    for a in articles {
        sqlx::query(
            "INSERT INTO news_article (digest_date, position, title, url, source, score, summary, key_points, image_url, read_minutes)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(date).bind(a.position).bind(&a.title).bind(&a.url).bind(&a.source)
            .bind(a.score).bind(&a.summary).bind(&a.key_points_json)
            .bind(&a.image_url).bind(a.read_minutes)
            .execute(&mut *tx).await?;
    }
    for q in quiz {
        sqlx::query(
            "INSERT INTO news_quiz_question (digest_date, position, article_pos, question, options, answer_index, explanation)
             VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(date).bind(q.position).bind(q.article_pos).bind(&q.question)
            .bind(&q.options_json).bind(q.answer_index).bind(&q.explanation)
            .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Articles for a date, ordered by position.
pub async fn articles(db: &Db, date: &str) -> anyhow::Result<Vec<ArticleRow>> {
    Ok(sqlx::query_as(
        "SELECT position, title, url, source, score, summary, key_points, image_url, read_minutes
         FROM news_article WHERE digest_date = ? ORDER BY position")
        .bind(date).fetch_all(db).await?)
}

/// Quiz questions for a date, ordered by position.
pub async fn quiz(db: &Db, date: &str) -> anyhow::Result<Vec<QuizRow>> {
    Ok(sqlx::query_as(
        "SELECT position, article_pos, question, options, answer_index, explanation
         FROM news_quiz_question WHERE digest_date = ? ORDER BY position")
        .bind(date).fetch_all(db).await?)
}

/// Distinct digest dates, newest first, with their article counts. Paginated.
pub async fn dates(db: &Db, limit: i64, offset: i64) -> anyhow::Result<Vec<DateRow>> {
    Ok(sqlx::query_as(
        "SELECT d.digest_date, d.created_at, COUNT(a.position) AS article_count
         FROM news_digest d
         LEFT JOIN news_article a ON a.digest_date = d.digest_date
         GROUP BY d.digest_date, d.created_at
         ORDER BY d.digest_date DESC
         LIMIT ? OFFSET ?")
        .bind(limit).bind(offset).fetch_all(db).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_then_read_roundtrips() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let arts = vec![NewArticle {
            position: 0, title: "Rust 2.0".into(), url: "https://ex.com/r".into(),
            source: "HN".into(), score: 100, summary: "ringkas".into(),
            key_points_json: "[\"a\",\"b\"]".into(),
            image_url: Some("https://ex.com/i.png".into()), read_minutes: Some(3),
        }];
        let quizzes = vec![NewQuiz {
            position: 0, article_pos: Some(0), question: "apa?".into(),
            options_json: "[\"x\",\"y\"]".into(), answer_index: 1, explanation: Some("krn".into()),
        }];
        insert(&db, "2026-06-16", "2026-06-16T00:00:00Z", &arts, &quizzes).await.unwrap();

        assert!(exists(&db, "2026-06-16").await.unwrap());
        let a = articles(&db, "2026-06-16").await.unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].title, "Rust 2.0");
        assert_eq!(a[0].image_url.as_deref(), Some("https://ex.com/i.png"));
        assert_eq!(a[0].read_minutes, Some(3));
        let q = quiz(&db, "2026-06-16").await.unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].answer_index, 1);
    }

    #[tokio::test]
    async fn dates_lists_newest_first_with_counts() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let art = |pos: i64| NewArticle {
            position: pos, title: "t".into(), url: format!("https://ex.com/{pos}"),
            source: "HN".into(), score: 1, summary: "s".into(),
            key_points_json: "[]".into(), image_url: None, read_minutes: None,
        };
        insert(&db, "2026-06-18", "2026-06-18T00:00:00Z", &[art(0)], &[]).await.unwrap();
        insert(&db, "2026-06-19", "2026-06-19T00:00:00Z", &[art(0), art(1)], &[]).await.unwrap();

        let all = dates(&db, 30, 0).await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].digest_date, "2026-06-19");
        assert_eq!(all[0].article_count, 2);
        assert_eq!(all[1].digest_date, "2026-06-18");
        assert_eq!(all[1].article_count, 1);

        // pagination: limit 1, offset 1 → the second-newest only
        let page = dates(&db, 1, 1).await.unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].digest_date, "2026-06-18");
    }
}
