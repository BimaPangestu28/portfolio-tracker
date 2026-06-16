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

pub struct NewArticle {
    pub position: i64,
    pub title: String,
    pub url: String,
    pub source: String,
    pub score: i64,
    pub summary: String,
    pub key_points_json: String,
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
            "INSERT INTO news_article (digest_date, position, title, url, source, score, summary, key_points)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(date).bind(a.position).bind(&a.title).bind(&a.url).bind(&a.source)
            .bind(a.score).bind(&a.summary).bind(&a.key_points_json)
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
        "SELECT position, title, url, source, score, summary, key_points
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
        let q = quiz(&db, "2026-06-16").await.unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].answer_index, 1);
    }
}
