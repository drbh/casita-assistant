//! Camera management and streaming support
//!
//! Supports multiple stream types:
//! - MJPEG: Direct HTTP proxy (most compatible)
//! - RTSP/H.264: Native handling via retina crate, served as fMP4 for browser MSE
//! - WebRTC: Planned for lowest latency (not yet implemented)

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use dashmap::DashMap;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::rtsp::{build_rtsp_url, Fmp4Writer, RtspClient};
use crate::{ApiResponse, AppState};

/// Camera stream type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StreamType {
    /// MJPEG stream (HTTP proxy, most compatible)
    Mjpeg,
    /// RTSP H.264 stream (native retina client, served as fMP4 for MSE)
    Rtsp,
    /// WebRTC stream (planned, not yet implemented)
    WebRtc,
}

/// Query parameters for stream endpoint
#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    /// Output format: "fmp4" (default for H.264), "mjpeg" (fallback)
    pub format: Option<String>,
}

/// Camera configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Camera {
    pub id: String,
    pub name: String,
    pub stream_url: String,
    pub stream_type: StreamType,
    pub enabled: bool,
    /// Optional RTSP username
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Optional RTSP password
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

/// Request to add a new camera
#[derive(Debug, Deserialize)]
pub struct AddCameraRequest {
    pub name: String,
    pub stream_url: String,
    #[serde(default = "default_stream_type")]
    pub stream_type: StreamType,
    /// Optional RTSP username
    pub username: Option<String>,
    /// Optional RTSP password
    pub password: Option<String>,
}

fn default_stream_type() -> StreamType {
    StreamType::Mjpeg
}

/// Request to update a camera
#[derive(Debug, Deserialize)]
pub struct UpdateCameraRequest {
    pub name: Option<String>,
    pub stream_url: Option<String>,
    pub stream_type: Option<StreamType>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub enabled: Option<bool>,
}

/// Camera manager for storing and retrieving cameras
pub struct CameraManager {
    cameras: Arc<DashMap<String, Camera>>,
    data_path: PathBuf,
}

impl CameraManager {
    /// Create a new camera manager
    pub fn new(data_dir: &std::path::Path) -> Self {
        Self {
            cameras: Arc::new(DashMap::new()),
            data_path: data_dir.join("cameras.json"),
        }
    }

    /// Load cameras from disk
    pub fn load(&self) -> anyhow::Result<()> {
        if self.data_path.exists() {
            let content = std::fs::read_to_string(&self.data_path)?;
            let cameras: Vec<Camera> = serde_json::from_str(&content)?;
            for camera in cameras {
                self.cameras.insert(camera.id.clone(), camera);
            }
            tracing::info!(
                "Loaded {} cameras from {:?}",
                self.cameras.len(),
                self.data_path
            );
        }
        Ok(())
    }

    /// Save cameras to disk
    pub fn save(&self) -> anyhow::Result<()> {
        let cameras: Vec<Camera> = self.cameras.iter().map(|r| r.value().clone()).collect();
        let content = serde_json::to_string_pretty(&cameras)?;

        // Ensure parent directory exists
        if let Some(parent) = self.data_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&self.data_path, content)?;
        tracing::debug!("Saved {} cameras to {:?}", cameras.len(), self.data_path);
        Ok(())
    }

    /// Add a new camera
    pub fn add(&self, camera: Camera) -> anyhow::Result<()> {
        self.cameras.insert(camera.id.clone(), camera);
        self.save()
    }

    /// Remove a camera by ID
    pub fn remove(&self, id: &str) -> Option<Camera> {
        let removed = self.cameras.remove(id).map(|(_, v)| v);
        if removed.is_some() {
            let _ = self.save();
        }
        removed
    }

    /// Get a camera by ID
    pub fn get(&self, id: &str) -> Option<Camera> {
        self.cameras.get(id).map(|r| r.value().clone())
    }

    /// Update a camera
    pub fn update(&self, id: &str, req: UpdateCameraRequest) -> Option<Camera> {
        let mut camera = self.cameras.get_mut(id)?;
        if let Some(name) = req.name {
            camera.name = name;
        }
        if let Some(stream_url) = req.stream_url {
            camera.stream_url = stream_url;
        }
        if let Some(stream_type) = req.stream_type {
            camera.stream_type = stream_type;
        }
        if let Some(username) = req.username {
            camera.username = Some(username);
        }
        if let Some(password) = req.password {
            camera.password = Some(password);
        }
        if let Some(enabled) = req.enabled {
            camera.enabled = enabled;
        }
        let updated = camera.clone();
        drop(camera);
        let _ = self.save();
        Some(updated)
    }

    /// List all cameras
    pub fn list(&self) -> Vec<Camera> {
        self.cameras.iter().map(|r| r.value().clone()).collect()
    }
}

// =============================================================================
// HTTP Handlers
// =============================================================================

/// List all cameras
pub async fn list_cameras(State(state): State<AppState>) -> impl IntoResponse {
    let cameras = state.cameras.list();
    Json(ApiResponse::success(cameras))
}

/// Add a new camera
pub async fn add_camera(
    State(state): State<AppState>,
    Json(req): Json<AddCameraRequest>,
) -> impl IntoResponse {
    let camera = Camera {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        stream_url: req.stream_url,
        stream_type: req.stream_type,
        enabled: true,
        username: req.username,
        password: req.password,
    };

    match state.cameras.add(camera.clone()) {
        Ok(_) => (StatusCode::CREATED, Json(ApiResponse::success(camera))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

/// Get a camera by ID
pub async fn get_camera(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.cameras.get(&id) {
        Some(camera) => (StatusCode::OK, Json(ApiResponse::success(camera))),
        None => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error("Camera not found")),
        ),
    }
}

/// Update a camera
pub async fn update_camera(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateCameraRequest>,
) -> impl IntoResponse {
    match state.cameras.update(&id, req) {
        Some(camera) => (StatusCode::OK, Json(ApiResponse::success(camera))),
        None => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error("Camera not found")),
        ),
    }
}

/// Delete a camera
pub async fn delete_camera(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.cameras.remove(&id) {
        Some(camera) => (StatusCode::OK, Json(ApiResponse::success(camera))),
        None => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error("Camera not found")),
        ),
    }
}

/// Proxy stream from camera
///
/// For MJPEG cameras: Direct HTTP proxy
/// For RTSP cameras: Native handling via retina, served as fMP4 for browser MSE
///
/// Query parameters:
/// - format: "fmp4" (default for RTSP), "mjpeg"
pub async fn stream_proxy(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<StreamQuery>,
) -> impl IntoResponse {
    // Look up camera
    let camera = match state.cameras.get(&id) {
        Some(c) => c,
        None => {
            return (StatusCode::NOT_FOUND, "Camera not found".to_string()).into_response();
        }
    };

    if !camera.enabled {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Camera is disabled".to_string(),
        )
            .into_response();
    }

    let format = query.format.as_deref().unwrap_or("auto");

    match camera.stream_type {
        StreamType::Mjpeg => stream_mjpeg(&camera).await,
        StreamType::Rtsp => {
            // For RTSP, default to fMP4 for efficient H.264 passthrough
            match format {
                "mjpeg" => {
                    // Fallback to MJPEG via transcoding (not recommended)
                    (
                        StatusCode::NOT_IMPLEMENTED,
                        "MJPEG transcoding from RTSP is no longer supported. Use fMP4 format.".to_string(),
                    )
                        .into_response()
                }
                _ => stream_rtsp_fmp4(&camera).await,
            }
        }
        StreamType::WebRtc => (
            StatusCode::NOT_IMPLEMENTED,
            "WebRTC streams are not yet supported via this endpoint".to_string(),
        )
            .into_response(),
    }
}

/// Stream MJPEG by proxying HTTP
async fn stream_mjpeg(camera: &Camera) -> axum::response::Response {
    tracing::info!(
        "Proxying MJPEG stream from {} for camera {}",
        camera.stream_url,
        camera.name
    );

    // Create HTTP client and fetch the stream
    let client = reqwest::Client::new();
    let response = match client.get(&camera.stream_url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Failed to connect to camera: {}", e);
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to connect to camera: {}", e),
            )
                .into_response();
        }
    };

    if !response.status().is_success() {
        return (
            StatusCode::BAD_GATEWAY,
            format!("Camera returned error: {}", response.status()),
        )
            .into_response();
    }

    // Get content-type from upstream (should be multipart/x-mixed-replace)
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("multipart/x-mixed-replace; boundary=frame")
        .to_string();

    // Convert the response body stream to an axum Body
    let stream = response.bytes_stream().map(|result| {
        result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    });

    let body = Body::from_stream(stream);

    // Return streaming response
    (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], body).into_response()
}

/// Stream RTSP via native retina client as fMP4 (fragmented MP4)
///
/// This is much more efficient than FFmpeg transcoding because:
/// 1. No external process needed (pure Rust)
/// 2. No transcoding - H.264 is passed through directly
/// 3. Browser can decode H.264 natively via MSE (Media Source Extensions)
async fn stream_rtsp_fmp4(camera: &Camera) -> axum::response::Response {
    // Build RTSP URL with credentials
    let rtsp_url = match build_rtsp_url(
        &camera.stream_url,
        camera.username.as_deref(),
        camera.password.as_deref(),
    ) {
        Ok(url) => url,
        Err(e) => {
            tracing::error!("Invalid RTSP URL: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                format!("Invalid RTSP URL: {}", e),
            )
                .into_response();
        }
    };

    tracing::info!(
        "Starting native RTSP stream for camera {} (fMP4 output)",
        camera.name
    );

    let camera_name = camera.name.clone();

    // Create the fMP4 stream
    let stream = async_stream::stream! {
        // Connect to RTSP stream using retina
        let client = RtspClient::new(rtsp_url);

        let (params, mut rx) = match client.connect().await {
            Ok(result) => result,
            Err(e) => {
                tracing::error!("Failed to connect to RTSP stream: {}", e);
                return;
            }
        };

        tracing::info!("Connected to RTSP stream for camera {}", camera_name);

        // Initialize fMP4 writer
        let mut writer = Fmp4Writer::new();

        // Default SPS/PPS for H.264 baseline profile (will be updated from stream)
        let default_sps = vec![0x67, 0x64, 0x00, 0x1f, 0xac, 0xd9, 0x40, 0x50, 0x05, 0xbb, 0x01, 0x10, 0x00, 0x00, 0x03, 0x00, 0x10, 0x00, 0x00, 0x03, 0x03, 0xc0, 0xf1, 0x83, 0x18, 0x46];
        let default_pps = vec![0x68, 0xeb, 0xe3, 0xcb, 0x22, 0xc0];

        // Send initialization segment
        let init_segment = writer.write_init_segment(
            params.width,
            params.height,
            if params.sps.is_empty() { &default_sps } else { &params.sps },
            if params.pps.is_empty() { &default_pps } else { &params.pps },
        );
        yield Ok::<_, std::io::Error>(init_segment);

        // Stream frames as fMP4 segments
        let mut frame_count = 0u64;
        let frame_duration = 3000u32; // ~33ms at 90kHz for 30fps

        loop {
            match rx.recv().await {
                Ok(frame) => {
                    // Only start streaming from keyframe for clean playback
                    if frame_count == 0 && !frame.is_keyframe {
                        continue;
                    }

                    let segment = writer.write_media_segment(
                        &frame.data,
                        frame.is_keyframe,
                        frame_duration,
                    );
                    yield Ok(segment);

                    frame_count += 1;

                    // Log periodically
                    if frame_count % 300 == 0 {
                        tracing::debug!("Streamed {} frames for camera {}", frame_count, camera_name);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Dropped {} frames due to slow consumer", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("RTSP stream closed for camera {}", camera_name);
                    break;
                }
            }
        }
    };

    let body = Body::from_stream(stream);

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "video/mp4".to_string())],
        body,
    )
        .into_response()
}
