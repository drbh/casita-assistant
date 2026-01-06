//! Action executor for automations

use crate::error::AutomationError;
use crate::model::{Action, DeviceCommand, LogLevel};
use std::sync::Arc;
use tokio::sync::broadcast;
use zigbee_core::ZigbeeNetwork;

/// Events emitted during action execution
#[derive(Debug, Clone)]
pub enum ExecutorEvent {
    /// Action started executing
    ActionStarted {
        automation_id: String,
        action_index: usize,
    },
    /// Action completed successfully
    ActionCompleted {
        automation_id: String,
        action_index: usize,
    },
    /// Action failed
    ActionFailed {
        automation_id: String,
        action_index: usize,
        error: String,
    },
}

/// Executor for automation actions
pub struct ActionExecutor {
    network: Option<Arc<ZigbeeNetwork>>,
    event_tx: broadcast::Sender<ExecutorEvent>,
}

impl ActionExecutor {
    /// Create a new action executor
    pub fn new(network: Option<Arc<ZigbeeNetwork>>) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self { network, event_tx }
    }

    /// Subscribe to executor events
    pub fn subscribe(&self) -> broadcast::Receiver<ExecutorEvent> {
        self.event_tx.subscribe()
    }

    /// Execute a list of actions for an automation
    pub async fn execute_actions(
        &self,
        automation_id: &str,
        actions: &[Action],
    ) -> Result<(), AutomationError> {
        for (index, action) in actions.iter().enumerate() {
            let _ = self.event_tx.send(ExecutorEvent::ActionStarted {
                automation_id: automation_id.to_string(),
                action_index: index,
            });

            match self.execute_action(action).await {
                Ok(()) => {
                    let _ = self.event_tx.send(ExecutorEvent::ActionCompleted {
                        automation_id: automation_id.to_string(),
                        action_index: index,
                    });
                }
                Err(e) => {
                    let _ = self.event_tx.send(ExecutorEvent::ActionFailed {
                        automation_id: automation_id.to_string(),
                        action_index: index,
                        error: e.to_string(),
                    });
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Execute a single action
    async fn execute_action(&self, action: &Action) -> Result<(), AutomationError> {
        match action {
            Action::DeviceControl {
                device_ieee,
                endpoint,
                command,
            } => {
                self.execute_device_control(device_ieee, *endpoint, command)
                    .await
            }
            Action::Delay { seconds } => {
                tracing::debug!("Delaying for {} seconds", seconds);
                tokio::time::sleep(std::time::Duration::from_secs(*seconds)).await;
                Ok(())
            }
            Action::TriggerAutomation { automation_id } => {
                // Note: Automation chaining is handled at the engine level
                // This action type should be intercepted by the engine before reaching here
                tracing::warn!(
                    "TriggerAutomation action for '{}' reached executor - this should be handled by the engine",
                    automation_id
                );
                Ok(())
            }
            Action::Log { message, level } => {
                self.execute_log(message, level);
                Ok(())
            }
        }
    }

    /// Execute a device control action
    async fn execute_device_control(
        &self,
        device_ieee: &str,
        endpoint: u8,
        command: &DeviceCommand,
    ) -> Result<(), AutomationError> {
        let network = self.network.as_ref().ok_or_else(|| {
            AutomationError::DeviceControlFailed("No network available".to_string())
        })?;

        let ieee = parse_ieee_address(device_ieee)?;

        let result = match command {
            DeviceCommand::TurnOn => network.turn_on(&ieee, endpoint).await,
            DeviceCommand::TurnOff => network.turn_off(&ieee, endpoint).await,
            DeviceCommand::Toggle => network.toggle_device(&ieee, endpoint).await,
        };

        result.map_err(|e| AutomationError::DeviceControlFailed(e.to_string()))
    }

    /// Execute a log action
    fn execute_log(&self, message: &str, level: &LogLevel) {
        match level {
            LogLevel::Debug => tracing::debug!(target: "automation", "{}", message),
            LogLevel::Info => tracing::info!(target: "automation", "{}", message),
            LogLevel::Warn => tracing::warn!(target: "automation", "{}", message),
            LogLevel::Error => tracing::error!(target: "automation", "{}", message),
        }
    }
}

/// Parse an IEEE address string (e.g., "00:11:22:33:44:55:66:77")
fn parse_ieee_address(s: &str) -> Result<[u8; 8], AutomationError> {
    let bytes: Vec<u8> = s
        .split(':')
        .map(|part| u8::from_str_radix(part, 16))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AutomationError::InvalidAction(format!("Invalid IEEE address: {}", s)))?;

    if bytes.len() != 8 {
        return Err(AutomationError::InvalidAction(format!(
            "IEEE address must have 8 bytes, got {}",
            bytes.len()
        )));
    }

    // Reverse to match internal representation (little-endian)
    let mut arr = [0u8; 8];
    for (i, &b) in bytes.iter().rev().enumerate() {
        arr[i] = b;
    }
    Ok(arr)
}
