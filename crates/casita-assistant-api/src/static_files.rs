//! Embedded static file serving for production builds
//!
//! This module embeds the frontend build output into the binary using rust-embed.
//! Only compiled when the `embed-frontend` feature is enabled.

use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../frontend/dist"]
struct Asset;

/// Serve an embedded file by path
pub async fn serve_embedded(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // Try to serve the exact path first
    if let Some(content) = Asset::get(path) {
        return serve_file(path, &content.data);
    }

    // For SPA routing: if path doesn't exist and isn't an asset, serve index.html
    // This allows client-side routing to work
    if !path.starts_with("assets/") && !path.contains('.') {
        if let Some(content) = Asset::get("index.html") {
            return serve_file("index.html", &content.data);
        }
    }

    // Not found - serve index.html for SPA routing
    match Asset::get("index.html") {
        Some(content) => serve_file("index.html", &content.data),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap(),
    }
}

/// Serve a file with appropriate headers
fn serve_file(path: &str, data: &[u8]) -> Response {
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    // Cache immutable assets (those with hashes in filename) forever
    // Don't cache index.html so updates are picked up
    let cache_control = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&mime)
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        )
        .header(header::CACHE_CONTROL, cache_control)
        .body(Body::from(data.to_vec()))
        .unwrap()
}
