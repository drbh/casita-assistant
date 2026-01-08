//! Camera management and streaming support

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use dashmap::DashMap;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

use crate::{ApiResponse, AppState};

/// Camera stream type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StreamType {
    Mjpeg,
    Rtsp,
    WebRtc,
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

/// Proxy stream from camera (MJPEG direct or RTSP via FFmpeg)
pub async fn stream_proxy(
    State(state): State<AppState>,
    Path(id): Path<String>,
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

    match camera.stream_type {
        StreamType::Mjpeg => stream_mjpeg(&camera).await,
        StreamType::Rtsp => stream_rtsp(&camera).await,
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

/// Stream RTSP via FFmpeg transcoding to MJPEG
async fn stream_rtsp(camera: &Camera) -> axum::response::Response {
    use crate::rtsp::RtspTranscoder;
    use tokio::io::AsyncReadExt;

    // Build RTSP URL with credentials
    let rtsp_url = RtspTranscoder::build_rtsp_url(
        &camera.stream_url,
        camera.username.as_deref(),
        camera.password.as_deref(),
    );

    tracing::info!("Starting RTSP stream via FFmpeg for camera {}", camera.name);

    // Clone URL for the stream closure
    let rtsp_url_clone = rtsp_url.clone();

    // Parse JPEG frames from FFmpeg output and wrap with multipart boundaries
    // The transcoder must be created inside the stream to keep it alive
    let stream = async_stream::stream! {
        // Create and start transcoder inside the stream
        let mut transcoder = RtspTranscoder::new();
        let stdout = match transcoder.start(&rtsp_url_clone).await {
            Ok(stdout) => stdout,
            Err(e) => {
                tracing::error!("Failed to start FFmpeg: {}", e);
                return;
            }
        };

        let mut reader = tokio::io::BufReader::with_capacity(64 * 1024, stdout);
        let mut buffer = Vec::with_capacity(512 * 1024);
        let mut chunk = [0u8; 32 * 1024]; // Read 32KB at a time
        let boundary = "frame";

        loop {
            // Read a chunk of data
            match reader.read(&mut chunk).await {
                Ok(0) => break, // EOF
                Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                Err(e) => {
                    tracing::warn!("FFmpeg read error: {}", e);
                    break;
                }
            }

            // Scan buffer for complete JPEG frames (FFD8...FFD9)
            loop {
                // Find JPEG start marker (FFD8)
                let start = buffer.iter().position(|&b| b == 0xFF)
                    .and_then(|i| {
                        if i + 1 < buffer.len() && buffer[i + 1] == 0xD8 {
                            Some(i)
                        } else {
                            None
                        }
                    });

                let start_idx = match start {
                    Some(idx) => idx,
                    None => break, // No JPEG start found
                };

                // Find JPEG end marker (FFD9) after start
                let end = buffer[start_idx..].windows(2)
                    .position(|w| w[0] == 0xFF && w[1] == 0xD9)
                    .map(|i| start_idx + i + 2);

                let end_idx = match end {
                    Some(idx) => idx,
                    None => break, // No complete frame yet
                };

                // Extract the complete JPEG frame
                let frame = &buffer[start_idx..end_idx];

                // Emit with multipart boundary
                let header = format!(
                    "--{}\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                    boundary,
                    frame.len()
                );
                yield Ok::<_, std::io::Error>(bytes::Bytes::from(header.into_bytes()));
                yield Ok(bytes::Bytes::copy_from_slice(frame));
                yield Ok(bytes::Bytes::from_static(b"\r\n"));

                // Remove processed data from buffer
                buffer.drain(..end_idx);
            }

            // Safety: limit buffer size
            if buffer.len() > 2 * 1024 * 1024 {
                buffer.clear();
            }
        }

        // Transcoder will be dropped here when stream ends
        drop(transcoder);
    };

    let body = Body::from_stream(stream);

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            format!("multipart/x-mixed-replace; boundary=frame"),
        )],
        body,
    )
        .into_response()
}
