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
    DeviceJoined {
        ieee_address: String,
    },
    DeviceLeft {
        ieee_address: String,
    },
    DeviceUpdated {
        ieee_address: String,
    },
    NetworkStateChanged {
        connected: bool,
    },
    // Device state events
    DeviceStateChanged {
        ieee_address: String,
        endpoint: u8,
        state_on: bool,
    },
    // Automation events
    AutomationTriggered {
        automation_id: String,
        trigger_reason: String,
    },
    AutomationActionExecuted {
        automation_id: String,
        action_index: usize,
    },
    AutomationFailed {
        automation_id: String,
        error: String,
    },
    AutomationCreated {
        automation_id: String,
    },
    AutomationUpdated {
        automation_id: String,
    },
    AutomationDeleted {
        automation_id: String,
    },
}

#[allow(clippy::too_many_lines)] // WebSocket handler manages multiple event sources
pub async fn handle_socket(socket: WebSocket, state: AppState) {
    let (sender, mut receiver) = socket.split();

    // Create a channel for aggregating events from multiple sources
    let (tx, mut rx) = tokio::sync::mpsc::channel::<WsEvent>(64);

    // Send connected message
    let tx_clone = tx.clone();
    let _ = tx_clone.send(WsEvent::Connected).await;

    // Spawn task to forward network events
    let network_task = if let Some(network) = &state.network {
        let mut event_rx = network.subscribe();
        let tx = tx.clone();
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
                                    ieee_address: format_ieee(ieee_address),
                                }
                            }
                            zigbee_core::network::NetworkEvent::DeviceUpdated { ieee_address } => {
                                WsEvent::DeviceUpdated {
                                    ieee_address: format_ieee(ieee_address),
                                }
                            }
                            zigbee_core::network::NetworkEvent::NetworkStateChanged {
                                connected,
                            } => WsEvent::NetworkStateChanged { connected },
                            zigbee_core::network::NetworkEvent::DeviceStateChanged {
                                ieee_address,
                                endpoint,
                                state_on,
                            } => WsEvent::DeviceStateChanged {
                                ieee_address: format_ieee(ieee_address),
                                endpoint,
                                state_on,
                            },
                        };

                        if tx.send(ws_event).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }))
    } else {
        None
    };

    // Spawn task to forward automation events
    let mut automation_rx = state.automations.subscribe();
    let automation_tx = tx.clone();
    let automation_task = tokio::spawn(async move {
        loop {
            match automation_rx.recv().await {
                Ok(event) => {
                    let ws_event = match event {
                        automation_engine::AutomationEvent::Triggered {
                            automation_id,
                            trigger_reason,
                        } => WsEvent::AutomationTriggered {
                            automation_id,
                            trigger_reason,
                        },
                        automation_engine::AutomationEvent::ActionExecuted {
                            automation_id,
                            action_index,
                        } => WsEvent::AutomationActionExecuted {
                            automation_id,
                            action_index,
                        },
                        automation_engine::AutomationEvent::Failed {
                            automation_id,
                            error,
                        } => WsEvent::AutomationFailed {
                            automation_id,
                            error,
                        },
                        automation_engine::AutomationEvent::Created { automation_id } => {
                            WsEvent::AutomationCreated { automation_id }
                        }
                        automation_engine::AutomationEvent::Updated { automation_id } => {
                            WsEvent::AutomationUpdated { automation_id }
                        }
                        automation_engine::AutomationEvent::Deleted { automation_id } => {
                            WsEvent::AutomationDeleted { automation_id }
                        }
                    };

                    if automation_tx.send(ws_event).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Spawn task to send aggregated events to WebSocket
    let sender = std::sync::Arc::new(tokio::sync::Mutex::new(sender));
    let sender_clone = sender.clone();
    let send_task = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let json = serde_json::to_string(&event).unwrap();
            let mut sender = sender_clone.lock().await;
            if sender.send(Message::Text(json)).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages (for future use)
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(_text)) => {
                // Handle client commands here if needed
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    // Clean up
    if let Some(task) = network_task {
        task.abort();
    }
    automation_task.abort();
    send_task.abort();
}

fn format_ieee(ieee: [u8; 8]) -> String {
    ieee.iter()
        .rev()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}
