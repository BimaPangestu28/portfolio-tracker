//! Customer-service knowledge base: chunking, embedding, and cosine retrieval.

/// Cosine similarity of two equal-length vectors. Returns 0.0 if either is empty
/// or has zero magnitude (defensive — avoids NaN from divide-by-zero).
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na  = 0.0f32;
    let mut nb  = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na  += a[i] * a[i];
        nb  += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Split a document body into retrieval chunks. Splits on blank lines
/// (paragraphs); paragraphs longer than `MAX_CHARS` are hard-split so no chunk
/// is too large to embed well. Empty/whitespace-only paragraphs are dropped.
pub fn chunk_text(body: &str) -> Vec<String> {
    const MAX_CHARS: usize = 1000;
    let mut chunks = Vec::new();
    for para in body.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        if para.chars().count() <= MAX_CHARS {
            chunks.push(para.to_string());
        } else {
            let chars: Vec<char> = para.chars().collect();
            for window in chars.chunks(MAX_CHARS) {
                chunks.push(window.iter().collect());
            }
        }
    }
    chunks
}

use crate::llm::claude::LlmError;

/// Abstraction over "turn texts into vectors" so the KB logic is testable with a
/// deterministic mock instead of a live API.
#[async_trait::async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, LlmError>;
}

/// OpenAI-shape embeddings client. Reuses OPENAI_API_KEY (the vision/ingest key)
/// and INGEST_BASE_URL; model defaults to text-embedding-3-small.
pub struct CsEmbedder {
    api_key: String,
    model:   String,
    base_url: String,
    client:  reqwest::Client,
}

impl CsEmbedder {
    pub fn from_env() -> Result<Self, LlmError> {
        let api_key  = std::env::var("OPENAI_API_KEY").map_err(|_| LlmError::NoKey)?;
        let model    = std::env::var("CS_EMBED_MODEL")
            .unwrap_or_else(|_| "text-embedding-3-small".into());
        let base_url = std::env::var("INGEST_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com".into())
            .trim_end_matches('/')
            .to_string();
        let client   = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| LlmError::Http(e.to_string()))?;
        Ok(Self { api_key, model, base_url, client })
    }
}

#[async_trait::async_trait]
impl Embedder for CsEmbedder {
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, LlmError> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let url  = format!("{}/v1/embeddings", self.base_url);
        let body = serde_json::json!({ "model": self.model, "input": inputs });
        let resp = self
            .client
            .post(&url)
            .header("authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;
        let status = resp.status();
        let json: serde_json::Value =
            resp.json().await.map_err(|e| LlmError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(LlmError::Api { status: status.as_u16(), body: json.to_string() });
        }
        parse_embeddings_response(&json)
    }
}

/// Extract the embedding vectors from an OpenAI `/v1/embeddings` response,
/// restoring request order via each item's `index`.
///
/// Assumes each item carries an `index` field (OpenAI always does). The
/// `unwrap_or(arrival position)` fallback only preserves order when items
/// arrive in the same order as the request — it is a safety net, not a
/// guarantee. If `index` is absent, callers should not rely on ordering.
pub fn parse_embeddings_response(json: &serde_json::Value) -> Result<Vec<Vec<f32>>, LlmError> {
    let data = json
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| LlmError::Shape("embeddings response missing 'data' array".into()))?;
    let mut indexed: Vec<(u64, Vec<f32>)> = Vec::with_capacity(data.len());
    for item in data {
        let index = item
            .get("index")
            .and_then(|i| i.as_u64())
            .unwrap_or(indexed.len() as u64);
        let emb = item
            .get("embedding")
            .and_then(|e| e.as_array())
            .ok_or_else(|| LlmError::Shape("embeddings item missing 'embedding'".into()))?;
        let vec: Vec<f32> = emb.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect();
        indexed.push((index, vec));
    }
    indexed.sort_by_key(|(i, _)| *i);
    Ok(indexed.into_iter().map(|(_, v)| v).collect())
}

use crate::db::Db;
use crate::repo::cs::KbChunkVec;

/// Embed every chunk that currently lacks an embedding. Returns how many were
/// embedded. Safe to call repeatedly (idempotent once all chunks are embedded).
pub async fn embed_pending<E: Embedder + ?Sized>(db: &Db, embedder: &E) -> anyhow::Result<usize> {
    let pending = crate::repo::cs::kb_chunks_without_embedding(db).await?;
    if pending.is_empty() {
        return Ok(0);
    }
    let texts: Vec<String> = pending.iter().map(|(_, t)| t.clone()).collect();
    let vectors = embedder
        .embed(&texts)
        .await
        .map_err(|e| anyhow::anyhow!("embed error: {e}"))?;
    if vectors.len() != pending.len() {
        anyhow::bail!(
            "embedder returned {} vectors for {} inputs",
            vectors.len(),
            pending.len()
        );
    }
    for ((chunk_id, _), vector) in pending.iter().zip(vectors.iter()) {
        let blob = crate::repo::cs::embedding_to_blob(vector);
        crate::repo::cs::kb_set_chunk_embedding(db, *chunk_id, &blob).await?;
    }
    Ok(pending.len())
}

/// Embed the query and return the `top_k` most cosine-similar chunks, best first.
pub async fn search<E: Embedder + ?Sized>(
    db: &Db,
    embedder: &E,
    query: &str,
    top_k: usize,
) -> anyhow::Result<Vec<KbChunkVec>> {
    let chunks = crate::repo::cs::kb_chunks_with_embedding(db).await?;
    if chunks.is_empty() {
        return Ok(Vec::new());
    }
    let q    = embedder
        .embed(&[query.to_string()])
        .await
        .map_err(|e| anyhow::anyhow!("embed error: {e}"))?;
    let qvec = q.into_iter().next().unwrap_or_default();
    if qvec.is_empty() {
        anyhow::bail!("embedder returned no vector for query");
    }
    let mut scored: Vec<(f32, KbChunkVec)> =
        chunks.into_iter().map(|c| (cosine(&qvec, &c.vector), c)).collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(scored.into_iter().take(top_k).map(|(_, c)| c).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identical_vectors_is_one() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal_is_zero() {
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
    }

    #[test]
    fn cosine_handles_degenerate_inputs() {
        assert_eq!(cosine(&[], &[]), 0.0);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
        assert_eq!(cosine(&[1.0, 2.0], &[1.0]), 0.0); // mismatched length
    }

    #[test]
    fn chunk_text_splits_paragraphs_and_drops_blanks() {
        let body   = "First para.\n\n   \n\nSecond para.";
        let chunks = chunk_text(body);
        assert_eq!(chunks, vec!["First para.".to_string(), "Second para.".to_string()]);
    }

    #[test]
    fn chunk_text_hard_splits_long_paragraph() {
        let long   = "a".repeat(2500);
        let chunks = chunk_text(&long);
        assert_eq!(chunks.len(), 3); // 1000 + 1000 + 500
        assert_eq!(chunks[0].chars().count(), 1000);
        assert_eq!(chunks[2].chars().count(), 500);
    }

    #[test]
    fn parse_embeddings_response_extracts_vectors_in_order() {
        let body = serde_json::json!({
            "object": "list",
            "data": [
                { "index": 0, "embedding": [0.1, 0.2] },
                { "index": 1, "embedding": [0.3, 0.4] }
            ]
        });
        let vecs = parse_embeddings_response(&body).unwrap();
        assert_eq!(vecs, vec![vec![0.1f32, 0.2], vec![0.3f32, 0.4]]);
    }

    #[test]
    fn parse_embeddings_response_sorts_by_index() {
        // API may return out-of-order; we must restore request order.
        let body = serde_json::json!({
            "data": [
                { "index": 1, "embedding": [9.0] },
                { "index": 0, "embedding": [1.0] }
            ]
        });
        let vecs = parse_embeddings_response(&body).unwrap();
        assert_eq!(vecs, vec![vec![1.0f32], vec![9.0f32]]);
    }

    struct MockEmbedder;
    #[async_trait::async_trait]
    impl Embedder for MockEmbedder {
        async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, crate::llm::claude::LlmError> {
            Ok(inputs
                .iter()
                .map(|s| {
                    let c = s.chars().next().unwrap_or(' ') as u32 as f32;
                    vec![c, (c * 2.0) % 7.0, 1.0]
                })
                .collect())
        }
    }

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn embed_pending_fills_missing_embeddings() {
        let db  = mem_db().await;
        let doc = crate::repo::cs::kb_doc_insert(&db, "Doc", None, "body").await.unwrap();
        crate::repo::cs::kb_replace_chunks(&db, doc, &["apple".into(), "banana".into()])
            .await
            .unwrap();

        let n = embed_pending(&db, &MockEmbedder).await.unwrap();
        assert_eq!(n, 2);
        assert!(crate::repo::cs::kb_chunks_without_embedding(&db).await.unwrap().is_empty());

        // idempotent: nothing left to embed
        assert_eq!(embed_pending(&db, &MockEmbedder).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn search_returns_most_similar_chunk_first() {
        let db  = mem_db().await;
        let doc = crate::repo::cs::kb_doc_insert(&db, "Doc", None, "body").await.unwrap();
        crate::repo::cs::kb_replace_chunks(&db, doc, &["apple pie".into(), "zebra".into()])
            .await
            .unwrap();
        embed_pending(&db, &MockEmbedder).await.unwrap();

        // query starting with 'a' embeds closest to "apple pie"
        let hits = search(&db, &MockEmbedder, "are you open?", 2).await.unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].text, "apple pie");
    }

    #[tokio::test]
    async fn search_with_empty_kb_returns_empty() {
        let db   = mem_db().await;
        let hits = search(&db, &MockEmbedder, "anything", 3).await.unwrap();
        assert!(hits.is_empty());
    }

    /// An embedder that always returns an empty vector list — simulates a broken
    /// embedder or a provider that returns no data for the query.
    struct EmptyEmbedder;
    #[async_trait::async_trait]
    impl Embedder for EmptyEmbedder {
        async fn embed(&self, _inputs: &[String]) -> Result<Vec<Vec<f32>>, crate::llm::claude::LlmError> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn search_errors_when_embedder_returns_empty_vector() {
        let db  = mem_db().await;
        // Insert a doc + chunk + embedding so KB is non-empty (search won't short-circuit).
        let doc = crate::repo::cs::kb_doc_insert(&db, "Doc", None, "body text").await.unwrap();
        crate::repo::cs::kb_replace_chunks(&db, doc, &["apple".into()])
            .await
            .unwrap();
        embed_pending(&db, &MockEmbedder).await.unwrap();

        let result = search(&db, &EmptyEmbedder, "anything", 3).await;
        assert!(result.is_err(), "expected error when embedder returns empty vec");
        let msg = result.err().unwrap().to_string();
        assert!(msg.contains("no vector"), "error message should mention 'no vector', got: {msg}");
    }
}
