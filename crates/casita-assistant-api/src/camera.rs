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

use crate::rtsp::{Fmp4Writer, RtspClient};
use crate::{ApiResponse, AppState};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StreamType {
    Mjpeg,
    Rtsp,
    WebRtc,
}

/// Query parameters for stream endpoint
#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    /// Output format: "fmp4" (default for H.264), "mjpeg" (fallback)
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Camera {
    pub id: String,
    pub name: String,
    pub stream_url: String,
    pub stream_type: StreamType,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddCameraRequest {
    pub name: String,
    pub stream_url: String,
    #[serde(default = "default_stream_type")]
    pub stream_type: StreamType,
    pub username: Option<String>,
    pub password: Option<String>,
}

fn default_stream_type() -> StreamType {
    StreamType::Mjpeg
}

#[derive(Debug, Deserialize)]
pub struct UpdateCameraRequest {
    pub name: Option<String>,
    pub stream_url: Option<String>,
    pub stream_type: Option<StreamType>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub enabled: Option<bool>,
}

pub struct CameraManager {
    cameras: Arc<DashMap<String, Camera>>,
    data_path: PathBuf,
}

impl CameraManager {
    pub fn new(data_dir: &std::path::Path) -> Self {
        Self {
            cameras: Arc::new(DashMap::new()),
            data_path: data_dir.join("cameras.json"),
        }
    }

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

    pub fn add(&self, camera: Camera) -> anyhow::Result<()> {
        self.cameras.insert(camera.id.clone(), camera);
        self.save()
    }

    pub fn remove(&self, id: &str) -> Option<Camera> {
        let removed = self.cameras.remove(id).map(|(_, v)| v);
        if removed.is_some() {
            let _ = self.save();
        }
        removed
    }

    pub fn get(&self, id: &str) -> Option<Camera> {
        self.cameras.get(id).map(|r| r.value().clone())
    }

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

    pub fn list(&self) -> Vec<Camera> {
        self.cameras.iter().map(|r| r.value().clone()).collect()
    }
}

// =============================================================================
// HTTP Handlers
// =============================================================================

pub async fn list_cameras(State(state): State<AppState>) -> impl IntoResponse {
    let cameras = state.cameras.list();
    Json(ApiResponse::success(cameras))
}

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
        Ok(()) => (StatusCode::CREATED, Json(ApiResponse::success(camera))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

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
                        "MJPEG transcoding from RTSP is no longer supported. Use fMP4 format."
                            .to_string(),
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

async fn stream_mjpeg(camera: &Camera) -> axum::response::Response {
    tracing::info!(
        "Proxying MJPEG stream from {} for camera {}",
        camera.stream_url,
        camera.name
    );

    let client = reqwest::Client::new();
    let response = match client.get(&camera.stream_url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Failed to connect to camera: {}", e);
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to connect to camera: {e}"),
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

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("multipart/x-mixed-replace; boundary=frame")
        .to_string();

    let stream = response
        .bytes_stream()
        .map(|result| result.map_err(|e| std::io::Error::other(e.to_string())));

    let body = Body::from_stream(stream);

    (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], body).into_response()
}

async fn stream_rtsp_fmp4(camera: &Camera) -> axum::response::Response {
    // Parse RTSP URL (without credentials - retina doesn't support embedded credentials)
    let rtsp_url = match url::Url::parse(&camera.stream_url) {
        Ok(url) => url,
        Err(e) => {
            tracing::error!("Invalid RTSP URL: {}", e);
            return (StatusCode::BAD_REQUEST, format!("Invalid RTSP URL: {e}")).into_response();
        }
    };

    tracing::info!(
        "Starting native RTSP stream for camera {} (fMP4 output)",
        camera.name
    );

    let camera_name = camera.name.clone();
    let username = camera.username.clone();
    let password = camera.password.clone();

    let stream = async_stream::stream! {
        let client = RtspClient::new(rtsp_url, username, password);

        let mut rx = match client.connect().await {
            Ok(result) => result,
            Err(e) => {
                tracing::error!("Failed to connect to RTSP stream: {}", e);
                return;
            }
        };

        tracing::info!("Connected to RTSP stream for camera {}", camera_name);

        let mut writer = Fmp4Writer::new();
        let mut init_sent = false;
        let mut frame_count = 0u64;
        let frame_duration = 3000u32; // ~33ms at 90kHz for 30fps

        loop {
            match rx.recv().await {
                Ok(frame) => {
                    // Wait for parameters before sending init segment
                    if !init_sent {
                        if let Some(params) = &frame.new_parameters {
                            let init_segment = writer.write_init_segment(
                                params.width,
                                params.height,
                                &params.avcc,
                            );
                            tracing::info!(
                                "Sending init segment for camera {} ({}x{}, avcc len={}, segment len={})",
                                camera_name, params.width, params.height, params.avcc.len(), init_segment.len()
                            );
                            yield Ok::<_, std::io::Error>(init_segment);
                            init_sent = true;
                        } else {
                            // Skip frames until we have parameters
                            continue;
                        }
                    }

                    // Only start streaming from keyframe for clean playback
                    if frame_count == 0 && !frame.is_keyframe {
                        continue;
                    }

                    let segment = writer.write_media_segment(
                        &frame.data,
                        frame.is_keyframe,
                        frame_duration,
                    );

                    frame_count += 1;

                    // Log first few segments and then periodically
                    if frame_count <= 3 || frame_count % 300 == 0 {
                        tracing::info!(
                            "Sending segment {} for camera {} (keyframe={}, data_len={}, segment_len={})",
                            frame_count, camera_name, frame.is_keyframe, frame.data.len(), segment.len()
                        );
                    }

                    yield Ok(segment);
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Dropped {} frames due to slow consumer", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("RTSP broadcast channel closed for camera {}", camera_name);
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
