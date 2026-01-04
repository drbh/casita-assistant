//! Casita Assistant - Zigbee Control API Server

use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use zigbee_core::ZigbeeNetwork;

mod websocket;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub network: Arc<ZigbeeNetwork>,
}

/// API response wrapper using serde_json::Value for flexibility
#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl ApiResponse {
    fn success<T: Serialize>(data: T) -> Self {
        Self {
            success: true,
            data: Some(serde_json::to_value(data).unwrap_or(serde_json::Value::Null)),
            error: None,
        }
    }

    fn error(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

/// System info response
#[derive(Serialize)]
struct SystemInfo {
    name: String,
    version: String,
    firmware: Option<String>,
}

/// Permit join request
#[derive(Deserialize)]
struct PermitJoinRequest {
    #[serde(default = "default_duration")]
    duration: u8,
}

fn default_duration() -> u8 {
    60
}

/// Get system info
async fn system_info(State(state): State<AppState>) -> impl IntoResponse {
    let firmware = match state.network.transport().get_version().await {
        Ok(v) => Some(v.to_string()),
        Err(_) => None,
    };

    Json(ApiResponse::success(SystemInfo {
        name: "Casita Assistant".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        firmware,
    }))
}

/// Get network status
async fn network_status(State(state): State<AppState>) -> impl IntoResponse {
    match state.network.get_status().await {
        Ok(status) => (StatusCode::OK, Json(ApiResponse::success(status))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

/// Permit devices to join
async fn permit_join(
    State(state): State<AppState>,
    Json(req): Json<PermitJoinRequest>,
) -> impl IntoResponse {
    match state.network.permit_join(req.duration).await {
        Ok(_) => (
            StatusCode::OK,
            Json(ApiResponse::success(serde_json::json!({
                "duration": req.duration
            }))),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

/// List all devices
async fn list_devices(State(state): State<AppState>) -> impl IntoResponse {
    let devices = state.network.get_devices();
    Json(ApiResponse::success(devices))
}

/// Get a specific device
async fn get_device(State(state): State<AppState>, Path(ieee): Path<String>) -> impl IntoResponse {
    // Parse IEEE address from hex string
    let ieee_bytes = match parse_ieee_address(&ieee) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("Invalid IEEE address format")),
            )
        }
    };

    match state.network.get_device(&ieee_bytes) {
        Some(device) => (StatusCode::OK, Json(ApiResponse::success(device))),
        None => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error("Device not found")),
        ),
    }
}

/// Discover endpoints for a device
async fn discover_device(
    State(state): State<AppState>,
    Path(ieee): Path<String>,
) -> impl IntoResponse {
    let ieee_bytes = match parse_ieee_address(&ieee) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("Invalid IEEE address format")),
            )
        }
    };

    match state.network.discover_endpoints(&ieee_bytes).await {
        Ok(_) => (
            StatusCode::OK,
            Json(ApiResponse::success(serde_json::json!({
                "status": "discovery_started",
                "ieee": ieee
            }))),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

/// Parse IEEE address from colon-separated hex string
fn parse_ieee_address(s: &str) -> Result<[u8; 8], ()> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 8 {
        // Try without colons
        if s.len() == 16 {
            let bytes: Result<Vec<u8>, _> = (0..8)
                .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16))
                .collect();
            if let Ok(b) = bytes {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&b);
                // Reverse for internal representation
                arr.reverse();
                return Ok(arr);
            }
        }
        return Err(());
    }

    let bytes: Result<Vec<u8>, _> = parts.iter().map(|p| u8::from_str_radix(p, 16)).collect();

    match bytes {
        Ok(b) if b.len() == 8 => {
            let mut arr = [0u8; 8];
            arr.copy_from_slice(&b);
            // Reverse because IEEE addresses are displayed in reverse byte order
            arr.reverse();
            Ok(arr)
        }
        _ => Err(()),
    }
}

/// WebSocket upgrade handler
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| websocket::handle_socket(socket, state))
}

/// Request APS data (fetch pending data from devices)
async fn request_aps_data(State(state): State<AppState>) -> impl IntoResponse {
    // First check if there's data waiting
    let device_state = match state.network.transport().get_device_state().await {
        Ok(state) => state,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            )
        }
    };

    if !device_state.aps_data_indication {
        return (
            StatusCode::OK,
            Json(ApiResponse::success(serde_json::json!({
                "status": "no_data",
                "message": "No APS data waiting"
            }))),
        );
    }

    match state.network.transport().request_aps_data().await {
        Ok(data) => {
            // Format the raw data as hex for visibility
            let hex_data: Vec<String> = data.iter().map(|b| format!("{b:02X}")).collect();
            (
                StatusCode::OK,
                Json(ApiResponse::success(serde_json::json!({
                    "status": "data_received",
                    "raw_data": hex_data,
                    "length": data.len()
                }))),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

/// Toggle device on/off
async fn toggle_device(
    State(state): State<AppState>,
    Path((ieee, endpoint)): Path<(String, u8)>,
) -> impl IntoResponse {
    let ieee_bytes = match parse_ieee_address(&ieee) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("Invalid IEEE address format")),
            )
        }
    };

    match state.network.toggle_device(&ieee_bytes, endpoint).await {
        Ok(_) => (
            StatusCode::OK,
            Json(ApiResponse::success(serde_json::json!({
                "action": "toggle",
                "ieee": ieee,
                "endpoint": endpoint
            }))),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

/// Turn device on
async fn device_on(
    State(state): State<AppState>,
    Path((ieee, endpoint)): Path<(String, u8)>,
) -> impl IntoResponse {
    let ieee_bytes = match parse_ieee_address(&ieee) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("Invalid IEEE address format")),
            )
        }
    };

    match state.network.turn_on(&ieee_bytes, endpoint).await {
        Ok(_) => (
            StatusCode::OK,
            Json(ApiResponse::success(serde_json::json!({
                "action": "on",
                "ieee": ieee,
                "endpoint": endpoint
            }))),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

/// Turn device off
async fn device_off(
    State(state): State<AppState>,
    Path((ieee, endpoint)): Path<(String, u8)>,
) -> impl IntoResponse {
    let ieee_bytes = match parse_ieee_address(&ieee) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("Invalid IEEE address format")),
            )
        }
    };

    match state.network.turn_off(&ieee_bytes, endpoint).await {
        Ok(_) => (
            StatusCode::OK,
            Json(ApiResponse::success(serde_json::json!({
                "action": "off",
                "ieee": ieee,
                "endpoint": endpoint
            }))),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

/// Health check
async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Serve the frontend
async fn index() -> Html<&'static str> {
    Html(include_str!("../../../webapp/index.html"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "casita_assistant_api=debug,deconz_protocol=debug,info".into()),
        )
        .init();

    tracing::info!("Starting Casita Assistant API server");

    // Get serial port from env or use default
    let serial_port = std::env::var("CONBEE_PORT").unwrap_or_else(|_| "/dev/ttyUSB0".to_string());

    // Connect to Zigbee network
    tracing::info!("Connecting to ConBee II at {}", serial_port);
    let network = ZigbeeNetwork::new(&serial_port).await?;

    // Query and display firmware version
    match network.transport().get_version().await {
        Ok(version) => tracing::info!("ConBee II firmware: {}", version),
        Err(e) => tracing::warn!("Failed to query firmware version: {}", e),
    }

    // Query network status
    match network.get_status().await {
        Ok(status) => {
            tracing::info!(
                "Network status: connected={}, channel={}, PAN ID={:#06x}",
                status.connected,
                status.channel,
                status.pan_id
            );
        }
        Err(e) => tracing::warn!("Failed to query network status: {}", e),
    }

    let state = AppState {
        network: Arc::new(network),
    };

    // Build the router
    let app = Router::new()
        // Frontend
        .route("/", get(index))
        .nest_service("/css", ServeDir::new("webapp/css"))
        .nest_service("/js", ServeDir::new("webapp/js"))
        // API routes
        .route("/health", get(health))
        .route("/api/v1/system/info", get(system_info))
        .route("/api/v1/network/status", get(network_status))
        .route("/api/v1/network/permit-join", post(permit_join))
        .route("/api/v1/network/aps-data", get(request_aps_data))
        .route("/api/v1/devices", get(list_devices))
        .route("/api/v1/devices/:ieee", get(get_device))
        .route("/api/v1/devices/:ieee/discover", post(discover_device))
        .route(
            "/api/v1/devices/:ieee/endpoints/:endpoint/toggle",
            post(toggle_device),
        )
        .route(
            "/api/v1/devices/:ieee/endpoints/:endpoint/on",
            post(device_on),
        )
        .route(
            "/api/v1/devices/:ieee/endpoints/:endpoint/off",
            post(device_off),
        )
        // WebSocket
        .route("/ws", get(ws_handler))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Start server
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
