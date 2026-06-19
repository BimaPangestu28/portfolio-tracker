use crate::error::AppError;
use crate::repo::news as repo;
use crate::AppState;
use axum::{extract::{Path, State}, Json};
use serde::Serialize;

#[derive(Serialize)]
pub struct ArticleDto {
    pub position: i64,
    pub title: String,
    pub url: String,
    pub source: String,
    pub summary: String,
    pub key_points: Vec<String>,
    pub image_url: Option<String>,
    pub read_minutes: Option<i64>,
}

#[derive(Serialize)]
pub struct QuizDto {
    pub position: i64,
    pub question: String,
    pub options: Vec<String>,
    pub answer_index: i64,
    pub explanation: Option<String>,
    pub article_position: Option<i64>,
}

#[derive(Serialize)]
pub struct TodayDto {
    pub available: bool,
    pub date: Option<String>,
    pub articles: Vec<ArticleDto>,
    pub quiz: Vec<QuizDto>,
}

/// Decode a stored JSON string-array column, warning (not silently defaulting)
/// when a row holds malformed JSON — mirrors `digest::load`'s handling.
fn decode_str_array(json: &str, field: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_else(|e| {
        tracing::warn!("news api: malformed {field} json: {e}");
        Vec::new()
    })
}

/// Shared core: build the digest DTO for a WIB date string. Returns
/// available:false with empty vecs when no articles exist for that date.
async fn load_digest(db: &crate::db::Db, date: &str) -> Result<TodayDto, AppError> {
    let articles = repo::articles(db, date).await.map_err(AppError::Other)?;
    if articles.is_empty() {
        return Ok(TodayDto { available: false, date: None, articles: vec![], quiz: vec![] });
    }
    let quiz = repo::quiz(db, date).await.map_err(AppError::Other)?;
    Ok(TodayDto {
        available: true,
        date: Some(date.to_string()),
        articles: articles
            .into_iter()
            .map(|a| ArticleDto {
                position: a.position,
                title: a.title,
                url: a.url,
                source: a.source,
                summary: a.summary,
                key_points: decode_str_array(&a.key_points, "key_points"),
                image_url: a.image_url,
                read_minutes: a.read_minutes,
            })
            .collect(),
        quiz: quiz
            .into_iter()
            .map(|q| QuizDto {
                position: q.position,
                question: q.question,
                options: decode_str_array(&q.options, "options"),
                answer_index: q.answer_index,
                explanation: q.explanation,
                article_position: q.article_pos,
            })
            .collect(),
    })
}

/// Read-only: today's persisted digest. Never triggers generation.
pub async fn today(State(s): State<AppState>) -> Result<Json<TodayDto>, AppError> {
    let date = chrono::Utc::now()
        .with_timezone(&crate::assistant::time::wib())
        .format("%Y-%m-%d")
        .to_string();
    Ok(Json(load_digest(&s.db, &date).await?))
}

/// Read-only: the persisted digest for a specific WIB date (YYYY-MM-DD).
pub async fn digest_by_date(
    State(s): State<AppState>,
    Path(date): Path<String>,
) -> Result<Json<TodayDto>, AppError> {
    chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest(format!("invalid date: {date}")))?;
    Ok(Json(load_digest(&s.db, &date).await?))
}

#[cfg(test)]
mod tests {
    use crate::AppState;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serial_test::serial;
    use tower::ServiceExt;

    async fn state_with_db() -> AppState {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        AppState {
            db,
            wa:          Default::default(),
            tg:          Default::default(),
            cs_wa:       Default::default(),
            cs_outbound: crate::cs::wa_outbound::new_queue(),
        }
    }

    fn today_wib() -> String {
        chrono::Utc::now().with_timezone(&crate::assistant::time::wib()).format("%Y-%m-%d").to_string()
    }

    #[serial]
    #[tokio::test]
    async fn today_returns_unavailable_when_no_digest() {
        let app = crate::api::router(state_with_db().await);
        let res = app
            .oneshot(Request::builder().uri("/news/today").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["available"], false);
        assert!(v["articles"].as_array().unwrap().is_empty());
    }

    #[serial]
    #[tokio::test]
    async fn today_returns_digest_when_present() {
        let state = state_with_db().await;
        let date = today_wib();
        crate::repo::news::insert(
            &state.db, &date, "2026-06-16T00:00:00Z",
            &[crate::repo::news::NewArticle {
                position: 0, title: "Rust 2.0".into(), url: "https://ex.com/r".into(),
                source: "HN".into(), score: 10, summary: "rilis".into(),
                key_points_json: "[\"cepat\"]".into(),
                image_url: Some("https://ex.com/i.png".into()), read_minutes: Some(4),
            }],
            &[crate::repo::news::NewQuiz {
                position: 0, article_pos: Some(0), question: "apa?".into(),
                options_json: "[\"x\",\"y\"]".into(), answer_index: 1, explanation: Some("krn".into()),
            }],
        ).await.unwrap();

        let app = crate::api::router(state);
        let res = app
            .oneshot(Request::builder().uri("/news/today").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["available"], true);
        assert_eq!(v["articles"][0]["title"], "Rust 2.0");
        assert_eq!(v["articles"][0]["key_points"][0], "cepat");
        assert_eq!(v["quiz"][0]["answer_index"], 1);
        assert_eq!(v["quiz"][0]["article_position"], 0);
        assert_eq!(v["articles"][0]["image_url"], "https://ex.com/i.png");
        assert_eq!(v["articles"][0]["read_minutes"], 4);
    }

    #[serial]
    #[tokio::test]
    async fn digest_by_date_returns_stored_digest() {
        let state = state_with_db().await;
        crate::repo::news::insert(
            &state.db, "2026-06-18", "2026-06-18T00:00:00Z",
            &[crate::repo::news::NewArticle {
                position: 0, title: "Rust 2.0".into(), url: "https://ex.com/r".into(),
                source: "HN".into(), score: 10, summary: "rilis".into(),
                key_points_json: "[\"cepat\"]".into(), image_url: None, read_minutes: Some(4),
            }],
            &[crate::repo::news::NewQuiz {
                position: 0, article_pos: Some(0), question: "apa?".into(),
                options_json: "[\"x\",\"y\"]".into(), answer_index: 1, explanation: None,
            }],
        ).await.unwrap();

        let app = crate::api::router(state);
        let res = app.oneshot(
            Request::builder().uri("/news/digest/2026-06-18").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["available"], true);
        assert_eq!(v["date"], "2026-06-18");
        assert_eq!(v["articles"][0]["title"], "Rust 2.0");
        assert_eq!(v["quiz"][0]["answer_index"], 1);
    }

    #[serial]
    #[tokio::test]
    async fn digest_by_date_rejects_malformed_date() {
        let app = crate::api::router(state_with_db().await);
        let res = app.oneshot(
            Request::builder().uri("/news/digest/not-a-date").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[serial]
    #[tokio::test]
    async fn digest_by_date_unknown_date_is_unavailable() {
        let app = crate::api::router(state_with_db().await);
        let res = app.oneshot(
            Request::builder().uri("/news/digest/2020-01-01").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["available"], false);
    }
}
