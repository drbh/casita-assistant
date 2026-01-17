//! Casita Assistant - Zigbee Control API Server

use automation_engine::{AutomationEngine, CreateAutomationRequest, UpdateAutomationRequest};
#[cfg(not(feature = "embed-frontend"))]
use axum::response::Html;
use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[cfg(not(feature = "embed-frontend"))]
use tower_http::services::ServeDir;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use zigbee_core::{DeviceCategory, ZigbeeNetwork};

mod camera;
mod rtsp;
#[cfg(feature = "embed-frontend")]
mod static_files;
mod websocket;

use camera::CameraManager;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub network: Option<Arc<ZigbeeNetwork>>,
    pub cameras: Arc<CameraManager>,
    pub automations: Arc<AutomationEngine>,
}

/// API response wrapper using `serde_json::Value` for flexibility
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
    let firmware = match &state.network {
        Some(network) => match network.transport().get_version().await {
            Ok(v) => Some(v.to_string()),
            Err(_) => None,
        },
        None => None,
    };

    Json(ApiResponse::success(SystemInfo {
        name: "Casita Assistant".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        firmware,
    }))
}

/// Get network status
async fn network_status(State(state): State<AppState>) -> impl IntoResponse {
    let Some(network) = &state.network else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::error("Zigbee network not available")),
        );
    };
    match network.get_status().await {
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
    let Some(network) = &state.network else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::error("Zigbee network not available")),
        );
    };
    match network.permit_join(req.duration).await {
        Ok(()) => (
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
    let devices = match &state.network {
        Some(network) => network.get_devices(),
        None => vec![],
    };
    Json(ApiResponse::success(devices))
}

/// Get a specific device
async fn get_device(State(state): State<AppState>, Path(ieee): Path<String>) -> impl IntoResponse {
    let Some(network) = &state.network else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::error("Zigbee network not available")),
        );
    };
    // Parse IEEE address from hex string
    let Ok(ieee_bytes) = parse_ieee_address(&ieee) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Invalid IEEE address format")),
        );
    };

    match network.get_device(&ieee_bytes) {
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
    let Some(network) = &state.network else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::error("Zigbee network not available")),
        );
    };
    let Ok(ieee_bytes) = parse_ieee_address(&ieee) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Invalid IEEE address format")),
        );
    };

    match network.discover_endpoints(&ieee_bytes).await {
        Ok(()) => (
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

/// Request body for updating device metadata
#[derive(Deserialize)]
struct UpdateDeviceRequest {
    #[serde(default)]
    friendly_name: Option<String>,
    #[serde(default)]
    category: Option<DeviceCategory>,
}

/// Update device metadata (friendly name and category)
async fn update_device(
    State(state): State<AppState>,
    Path(ieee): Path<String>,
    Json(request): Json<UpdateDeviceRequest>,
) -> impl IntoResponse {
    let Some(network) = &state.network else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::error("Zigbee network not available")),
        );
    };
    let Ok(ieee_bytes) = parse_ieee_address(&ieee) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Invalid IEEE address format")),
        );
    };

    match network.update_device_metadata(&ieee_bytes, request.friendly_name, request.category) {
        Ok(device) => (StatusCode::OK, Json(ApiResponse::success(device))),
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(ApiResponse::error(e.to_string())))
        }
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
    let Some(network) = &state.network else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::error("Zigbee network not available")),
        );
    };
    // First check if there's data waiting
    let device_state = match network.transport().get_device_state().await {
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

    match network.transport().request_aps_data().await {
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
    let Some(network) = &state.network else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::error("Zigbee network not available")),
        );
    };
    let Ok(ieee_bytes) = parse_ieee_address(&ieee) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Invalid IEEE address format")),
        );
    };

    match network.toggle_device(&ieee_bytes, endpoint).await {
        Ok(()) => (
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
    let Some(network) = &state.network else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::error("Zigbee network not available")),
        );
    };
    let Ok(ieee_bytes) = parse_ieee_address(&ieee) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Invalid IEEE address format")),
        );
    };

    match network.turn_on(&ieee_bytes, endpoint).await {
        Ok(()) => (
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
    let Some(network) = &state.network else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiResponse::error("Zigbee network not available")),
        );
    };
    let Ok(ieee_bytes) = parse_ieee_address(&ieee) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Invalid IEEE address format")),
        );
    };

    match network.turn_off(&ieee_bytes, endpoint).await {
        Ok(()) => (
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

// ============================================================================
// Automation handlers
// ============================================================================

/// List all automations
async fn list_automations(State(state): State<AppState>) -> impl IntoResponse {
    let automations = state.automations.list();
    Json(ApiResponse::success(automations))
}

/// Get a specific automation
async fn get_automation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.automations.get(&id) {
        Some(automation) => (StatusCode::OK, Json(ApiResponse::success(automation))),
        None => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error("Automation not found")),
        ),
    }
}

/// Create a new automation
async fn create_automation(
    State(state): State<AppState>,
    Json(request): Json<CreateAutomationRequest>,
) -> impl IntoResponse {
    match state.automations.create(request).await {
        Ok(automation) => (StatusCode::CREATED, Json(ApiResponse::success(automation))),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

/// Update an automation
async fn update_automation(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateAutomationRequest>,
) -> impl IntoResponse {
    match state.automations.update(&id, request).await {
        Ok(automation) => (StatusCode::OK, Json(ApiResponse::success(automation))),
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Delete an automation
async fn delete_automation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.automations.delete(&id).await {
        Ok(automation) => (StatusCode::OK, Json(ApiResponse::success(automation))),
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Manually trigger an automation
async fn trigger_automation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.automations.trigger(&id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success(serde_json::json!({
                "status": "triggered",
                "automation_id": id
            }))),
        ),
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else if e.to_string().contains("disabled") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Enable an automation
async fn enable_automation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.automations.enable(&id).await {
        Ok(automation) => (StatusCode::OK, Json(ApiResponse::success(automation))),
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Disable an automation
async fn disable_automation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.automations.disable(&id).await {
        Ok(automation) => (StatusCode::OK, Json(ApiResponse::success(automation))),
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Serve the frontend (legacy mode - for development with vanilla JS)
#[cfg(not(feature = "embed-frontend"))]
async fn index() -> Html<&'static str> {
    Html(include_str!("../../../webapp/index.html"))
}

#[tokio::main]
#[allow(clippy::too_many_lines)] // Application setup and routing configuration
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "casita_assistant_api=debug,deconz_protocol=debug,retina=error,info".into()
            }),
        )
        .init();

    tracing::info!("Starting Casita Assistant API server");

    // Initialize camera manager first (always available)
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    let cameras = CameraManager::new(std::path::Path::new(&data_dir));
    if let Err(e) = cameras.load() {
        tracing::warn!("Failed to load cameras: {}", e);
    }

    // Try to connect to Zigbee network (optional)
    let network = {
        // Get serial port from env or use default
        let serial_port = std::env::var("CONBEE_PORT").unwrap_or_else(|_| {
            // Try udev symlink first, then common paths
            for path in ["/dev/conbee2", "/dev/ttyACM0", "/dev/ttyUSB0"] {
                if std::path::Path::new(path).exists() {
                    return path.to_string();
                }
            }
            String::new()
        });

        if serial_port.is_empty() {
            tracing::warn!("No Zigbee device found - running without Zigbee support");
            None
        } else {
            tracing::info!("Connecting to ConBee II at {}", serial_port);
            match ZigbeeNetwork::new(&serial_port).await {
                Ok(network) => {
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
                    Some(Arc::new(network))
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to connect to Zigbee device: {} - running without Zigbee support",
                        e
                    );
                    None
                }
            }
        }
    };

    // Initialize automation engine
    let automations =
        match AutomationEngine::new(network.clone(), std::path::Path::new(&data_dir)).await {
            Ok(engine) => {
                let engine = Arc::new(engine);
                engine.start();
                tracing::info!(
                    "Automation engine started with {} automations",
                    engine.list().len()
                );
                engine
            }
            Err(e) => {
                tracing::error!("Failed to initialize automation engine: {}", e);
                return Err(anyhow::anyhow!(
                    "Failed to initialize automation engine: {e}"
                ));
            }
        };

    let state = AppState {
        network,
        cameras: Arc::new(cameras),
        automations,
    };

    // Build the router - API routes first (take priority over frontend)
    let app = Router::new()
        // API routes
        .route("/health", get(health))
        .route("/api/v1/system/info", get(system_info))
        .route("/api/v1/network/status", get(network_status))
        .route("/api/v1/network/permit-join", post(permit_join))
        .route("/api/v1/network/aps-data", get(request_aps_data))
        .route("/api/v1/devices", get(list_devices))
        .route("/api/v1/devices/:ieee", get(get_device))
        .route("/api/v1/devices/:ieee", axum::routing::put(update_device))
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
        // Camera routes
        .route("/api/v1/cameras", get(camera::list_cameras))
        .route("/api/v1/cameras", post(camera::add_camera))
        .route("/api/v1/cameras/:id", get(camera::get_camera))
        .route(
            "/api/v1/cameras/:id",
            axum::routing::put(camera::update_camera),
        )
        .route(
            "/api/v1/cameras/:id",
            axum::routing::delete(camera::delete_camera),
        )
        .route("/api/v1/cameras/:id/stream", get(camera::stream_proxy))
        // Automation routes
        .route("/api/v1/automations", get(list_automations))
        .route("/api/v1/automations", post(create_automation))
        .route("/api/v1/automations/:id", get(get_automation))
        .route(
            "/api/v1/automations/:id",
            axum::routing::put(update_automation),
        )
        .route(
            "/api/v1/automations/:id",
            axum::routing::delete(delete_automation),
        )
        .route("/api/v1/automations/:id/trigger", post(trigger_automation))
        .route("/api/v1/automations/:id/enable", post(enable_automation))
        .route("/api/v1/automations/:id/disable", post(disable_automation))
        // WebSocket
        .route("/ws", get(ws_handler))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Add frontend serving based on feature flags
    #[cfg(feature = "embed-frontend")]
    let app = {
        tracing::info!("Serving embedded frontend assets");
        app.fallback(static_files::serve_embedded)
    };

    #[cfg(not(feature = "embed-frontend"))]
    let app = {
        tracing::info!("Serving frontend from filesystem (legacy mode)");
        app.route("/", get(index))
            .nest_service("/css", ServeDir::new("webapp/css"))
            .nest_service("/js", ServeDir::new("webapp/js"))
    };

    // Start server
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
