//! REST API wrapper for cloud/serverless deployment using axum.
//!
//! Provides HTTP endpoints for PDF generation, merge, split, search, redaction,
//! and text extraction. Designed for deployment behind a reverse proxy or as
//! a serverless function.
//!
//! ## Endpoints
//!
//! | Method | Path | Description |
//! |--------|------|-------------|
//! | POST | `/api/v1/generate` | Generate PDF from Markdown |
//! | POST | `/api/v1/merge` | Merge multiple PDFs |
//! | POST | `/api/v1/split` | Split PDF into pages |
//! | POST | `/api/v1/search` | Search text in PDF |
//! | POST | `/api/v1/redact` | Redact regions from PDF |
//! | POST | `/api/v1/extract` | Extract text from PDF |
//! | GET  | `/api/v1/health` | Health check |
//!
//! ## Example
//!
//! ```rust,no_run
//! use pdfrs::api;
//!
//! # #[tokio::main]
//! # async fn main() {
//! api::serve("0.0.0.0", 8080).await;
//! # }
//! ```

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Maximum request body size in bytes (default 50 MB).
    pub max_body: usize,
}

impl Default for AppState {
    fn default() -> Self {
        Self { max_body: 50 * 1024 * 1024 }
    }
}

// ----- Request/Response types -----------------------------------------------

#[derive(Deserialize)]
pub struct GenerateRequest {
    pub markdown: String,
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_true")]
    pub portrait: bool,
}

fn default_font() -> String { "Helvetica".into() }
fn default_font_size() -> f32 { 12.0 }
fn default_true() -> bool { true }

#[derive(Serialize)]
pub struct PdfResponse {
    pub size: usize,
}

#[derive(Deserialize)]
pub struct MergeRequest {
    /// Base64-encoded PDFs to merge (in order).
    pub pdfs: Vec<String>,
}

#[derive(Deserialize)]
pub struct SplitRequest {
    /// Base64-encoded PDF.
    pub pdf: String,
}

#[derive(Serialize)]
pub struct SplitResponse {
    /// Number of pages extracted.
    pub pages: usize,
}

#[derive(Deserialize)]
pub struct SearchRequest {
    /// Base64-encoded PDF.
    pub pdf: String,
    pub query: String,
}

#[derive(Serialize)]
pub struct SearchResponse {
    pub total_hits: usize,
    pub hits: Vec<SearchHitDto>,
}

#[derive(Serialize)]
pub struct SearchHitDto {
    pub page: usize,
    pub snippet: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Deserialize)]
pub struct RedactRequest {
    /// Base64-encoded PDF.
    pub pdf: String,
    pub regions: Vec<RedactRegionDto>,
}

#[derive(Deserialize)]
pub struct RedactRegionDto {
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Deserialize)]
pub struct ExtractRequest {
    /// Base64-encoded PDF.
    pub pdf: String,
}

#[derive(Serialize)]
pub struct ExtractResponse {
    pub text: String,
    pub pages: usize,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ----- Handlers --------------------------------------------------------------

async fn health() -> &'static str {
    "ok"
}

async fn generate_pdf(
    State(_state): State<AppState>,
    Json(req): Json<GenerateRequest>,
) -> Response {
    let elements = crate::elements::parse_markdown(&req.markdown);
    let layout = if req.portrait {
        crate::pdf_generator::PageLayout::portrait()
    } else {
        crate::pdf_generator::PageLayout::landscape()
    };
    match crate::pdf_generator::generate_pdf_bytes(&elements, &req.font, req.font_size, layout) {
        Ok(pdf_bytes) => {
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/pdf")],
                pdf_bytes,
            )
                .into_response()
        }
        Err(e) => {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })).into_response()
        }
    }
}

async fn merge_pdfs(
    State(_state): State<AppState>,
    Json(req): Json<MergeRequest>,
) -> Response {
    use base64::{Engine, engine::general_purpose};
    let mut pdfs = Vec::new();
    for b64 in &req.pdfs {
        match general_purpose::STANDARD.decode(b64) {
            Ok(data) => pdfs.push(data),
            Err(e) => {
                return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid base64: {e}") })).into_response();
            }
        }
    }
    match crate::pdf_ops::merge_pdfs_from_bytes(&pdfs) {
        Ok(merged) => {
            (StatusCode::OK, [(header::CONTENT_TYPE, "application/pdf")], merged).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() })).into_response()
        }
    }
}

async fn split_pdf(
    State(_state): State<AppState>,
    Json(req): Json<SplitRequest>,
) -> Response {
    use base64::{Engine, engine::general_purpose};
    let pdf = match general_purpose::STANDARD.decode(&req.pdf) {
        Ok(d) => d,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid base64: {e}") })).into_response(),
    };
    match crate::pdf_ops::split_pdf_from_bytes(&pdf) {
        Ok(pages) => {
            // Return each page as base64 in a JSON array
            let encoded: Vec<String> = pages
                .iter()
                .map(|p| general_purpose::STANDARD.encode(p))
                .collect();
            (StatusCode::OK, Json(serde_json::json!({ "pages": encoded }))).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() })).into_response()
        }
    }
}

async fn search_pdf(
    State(_state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Response {
    use base64::{Engine, engine::general_purpose};
    let pdf = match general_purpose::STANDARD.decode(&req.pdf) {
        Ok(d) => d,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid base64: {e}") })).into_response(),
    };
    let hits = crate::search::search_text(&pdf, &req.query, false);
    let total = hits.len();
    let dtos: Vec<SearchHitDto> = hits
        .iter()
        .map(|h| SearchHitDto {
            page: h.page,
            snippet: h.snippet.clone(),
            x: h.bbox.x,
            y: h.bbox.y,
            width: h.bbox.width,
            height: h.bbox.height,
        })
        .collect();
    (StatusCode::OK, Json(SearchResponse { total_hits: total, hits: dtos })).into_response()
}

async fn redact_pdf(
    State(_state): State<AppState>,
    Json(req): Json<RedactRequest>,
) -> Response {
    use base64::{Engine, engine::general_purpose};
    let pdf = match general_purpose::STANDARD.decode(&req.pdf) {
        Ok(d) => d,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid base64: {e}") })).into_response(),
    };
    let regions: Vec<crate::redact::RedactionRegion> = req
        .regions
        .iter()
        .map(|r| crate::redact::RedactionRegion {
            page: r.page,
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        })
        .collect();
    match crate::redact::redact_pdf_bytes(&pdf, &regions) {
        Ok(redacted) => {
            (StatusCode::OK, [(header::CONTENT_TYPE, "application/pdf")], redacted).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() })).into_response()
        }
    }
}

async fn extract_text(
    State(_state): State<AppState>,
    Json(req): Json<ExtractRequest>,
) -> Response {
    use base64::{Engine, engine::general_purpose};
    let pdf = match general_purpose::STANDARD.decode(&req.pdf) {
        Ok(d) => d,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: format!("Invalid base64: {e}") })).into_response(),
    };
    let doc = match crate::pdf::PdfDocument::load_from_bytes(&pdf) {
        Ok(d) => d,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })).into_response(),
    };
    let text = doc.get_text().unwrap_or_default();
    let pages = crate::search::collect_pages_from_doc(&doc, Some(&pdf)).len();
    (StatusCode::OK, Json(ExtractResponse { text, pages })).into_response()
}

// ----- Server setup ----------------------------------------------------------

/// Build the axum router with all endpoints.
pub fn router() -> Router {
    router_with_state(AppState::default())
}

/// Build the router with custom application state.
pub fn router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/generate", post(generate_pdf))
        .route("/api/v1/merge", post(merge_pdfs))
        .route("/api/v1/split", post(split_pdf))
        .route("/api/v1/search", post(search_pdf))
        .route("/api/v1/redact", post(redact_pdf))
        .route("/api/v1/extract", post(extract_text))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Start the API server on the given host and port.
///
/// ```rust,no_run
/// # #[tokio::main]
/// # async fn main() {
/// pdfrs::api::serve("0.0.0.0", 8080).await;
/// # }
/// ```
pub async fn serve(host: &str, port: u16) {
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap_or_else(|e| {
        eprintln!("Failed to bind {addr}: {e}");
        std::process::exit(1);
    });
    println!("pdfrs API listening on http://{addr}");
    axum::serve(listener, router()).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use base64::Engine;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = router();
        let resp = app
            .oneshot(Request::builder().uri("/api/v1/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_generate_endpoint() {
        let app = router();
        let body = serde_json::json!({
            "markdown": "# Hello\n\nWorld",
            "font": "Helvetica",
            "font_size": 12.0,
            "portrait": true
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/generate")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/pdf"
        );
    }

    #[tokio::test]
    async fn test_extract_endpoint() {
        // Generate a small PDF first
        let elements = crate::elements::parse_markdown("# Test\n\nExtract me");
        let layout = crate::pdf_generator::PageLayout::portrait();
        let pdf_bytes = crate::pdf_generator::generate_pdf_bytes(&elements, "Helvetica", 12.0, layout).unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&pdf_bytes);

        let app = router();
        let body = serde_json::json!({ "pdf": b64 });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/extract")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
