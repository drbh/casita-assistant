//! WebSocket handler for real-time updates

use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use serde::Serialize;

use crate::AppState;

/// WebSocket events sent to clients
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    Connected,
    DeviceJoined { ieee_address: String },
    DeviceLeft { ieee_address: String },
    DeviceUpdated { ieee_address: String },
    NetworkStateChanged { connected: bool },
}

/// Handle a WebSocket connection
pub async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Send connected message
    let connected_msg = serde_json::to_string(&WsEvent::Connected).unwrap();
    if sender.send(Message::Text(connected_msg)).await.is_err() {
        return;
    }

    // Spawn task to forward network events to WebSocket (only if network is available)
    let send_task = if let Some(network) = &state.network {
        let mut event_rx = network.subscribe();
        Some(tokio::spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        let ws_event = match event {
                            zigbee_core::network::NetworkEvent::DeviceJoined(device) => {
                                WsEvent::DeviceJoined {
                                    ieee_address: device.ieee_address_string(),
                                }
                            }
                            zigbee_core::network::NetworkEvent::DeviceLeft { ieee_address } => {
                                WsEvent::DeviceLeft {
                                    ieee_address: format_ieee(&ieee_address),
                                }
                            }
                            zigbee_core::network::NetworkEvent::DeviceUpdated { ieee_address } => {
                                WsEvent::DeviceUpdated {
                                    ieee_address: format_ieee(&ieee_address),
                                }
                            }
                            zigbee_core::network::NetworkEvent::NetworkStateChanged {
                                connected,
                            } => WsEvent::NetworkStateChanged { connected },
                        };

                        let json = serde_json::to_string(&ws_event).unwrap();
                        if sender.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Skip missed messages
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        }))
    } else {
        None
    };

    // Handle incoming messages (for future use)
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(_text)) => {
                // Handle client commands here if needed
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    // Clean up
    if let Some(task) = send_task {
        task.abort();
    }
}

fn format_ieee(ieee: &[u8; 8]) -> String {
    ieee.iter()
        .rev()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}
