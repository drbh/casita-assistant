//! Error types for the automation engine

use thiserror::Error;

/// Errors that can occur in the automation engine
#[derive(Error, Debug)]
pub enum AutomationError {
    /// Automation not found
    #[error("Automation not found: {0}")]
    NotFound(String),

    /// Automation is disabled
    #[error("Automation is disabled: {0}")]
    Disabled(String),

    /// Invalid trigger configuration
    #[error("Invalid trigger: {0}")]
    InvalidTrigger(String),

    /// Invalid condition configuration
    #[error("Invalid condition: {0}")]
    InvalidCondition(String),

    /// Invalid action configuration
    #[error("Invalid action: {0}")]
    InvalidAction(String),

    /// Invalid cron expression
    #[error("Invalid cron expression: {0}")]
    InvalidCron(String),

    /// Invalid time format
    #[error("Invalid time format: {0}")]
    InvalidTimeFormat(String),

    /// Device not found for action
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    /// Device control failed
    #[error("Device control failed: {0}")]
    DeviceControlFailed(String),

    /// Circular automation reference detected
    #[error("Circular automation reference detected: {0}")]
    CircularReference(String),

    /// IO error (persistence)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Network error from zigbee-core
    #[error("Network error: {0}")]
    Network(String),
}
