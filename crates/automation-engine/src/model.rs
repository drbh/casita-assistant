//! Data models for the automation engine

use serde::{Deserialize, Serialize};

/// A complete automation rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Automation {
    /// Unique identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    /// Whether the automation is active
    pub enabled: bool,
    /// What initiates the automation
    pub trigger: Trigger,
    /// Optional additional conditions that must be true
    #[serde(default)]
    pub conditions: Vec<Condition>,
    /// Actions to execute when triggered and conditions are met
    pub actions: Vec<Action>,
    /// Creation timestamp (ISO 8601)
    pub created_at: String,
    /// Last modification timestamp
    pub updated_at: String,
}

/// Trigger types that can initiate an automation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// Device state change trigger
    DeviceState {
        /// IEEE address of the device (e.g., "00:11:22:33:44:55:66:77")
        device_ieee: String,
        /// Optional endpoint filter
        #[serde(default)]
        endpoint: Option<u8>,
        /// State change to watch for
        state_change: StateChange,
    },
    /// Time-based schedule trigger
    Schedule {
        /// Schedule specification
        schedule: ScheduleSpec,
    },
    /// Manual trigger (API call only)
    Manual,
}

/// State changes to monitor for device triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StateChange {
    /// Any state update from the device
    Any,
    /// Device became available
    Available,
    /// Device became unavailable
    Unavailable,
    /// Device joined the network
    Joined,
    /// Device left the network
    Left,
    /// Device was turned on
    TurnedOn,
    /// Device was turned off
    TurnedOff,
    /// Device state toggled (either on->off or off->on)
    Toggled,
}

/// Schedule specification for time-based triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScheduleSpec {
    /// Run at specific time(s) of day
    TimeOfDay {
        /// Time in HH:MM format (24-hour)
        time: String,
        /// Days of week (0=Sunday, 1=Monday, ..., 6=Saturday)
        /// Empty means every day
        #[serde(default)]
        days: Vec<u8>,
    },
    /// Run at fixed interval
    Interval {
        /// Interval in seconds
        seconds: u64,
    },
    /// Cron expression (advanced)
    Cron {
        /// Standard cron expression (e.g., "0 30 9 * * *" for 9:30 AM daily)
        expression: String,
    },
}

/// Conditions that must be true for actions to execute
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    /// Time range condition (actions only run within this time window)
    TimeRange {
        /// Start time in HH:MM format
        start: String,
        /// End time in HH:MM format (can wrap past midnight)
        end: String,
    },
    /// Day of week condition
    DayOfWeek {
        /// Days when condition is true (0=Sunday)
        days: Vec<u8>,
    },
    /// Device availability condition
    DeviceAvailable {
        /// IEEE address of the device
        device_ieee: String,
        /// Whether device should be available (true) or unavailable (false)
        available: bool,
    },
    /// Logical AND of multiple conditions
    And { conditions: Vec<Condition> },
    /// Logical OR of multiple conditions
    Or { conditions: Vec<Condition> },
    /// Negate a condition
    Not { condition: Box<Condition> },
}

/// Actions to perform when automation triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Control a device
    DeviceControl {
        /// IEEE address of the device
        device_ieee: String,
        /// Endpoint number
        endpoint: u8,
        /// Command to execute
        command: DeviceCommand,
    },
    /// Delay before next action
    Delay {
        /// Delay in seconds
        seconds: u64,
    },
    /// Trigger another automation (for chaining)
    TriggerAutomation {
        /// ID of automation to trigger
        automation_id: String,
    },
    /// Log a message (for debugging)
    Log {
        /// Message to log
        message: String,
        /// Log level
        #[serde(default)]
        level: LogLevel,
    },
}

/// Device commands for control actions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeviceCommand {
    /// Turn device on
    TurnOn,
    /// Turn device off
    TurnOff,
    /// Toggle device state
    Toggle,
}

/// Log levels for log actions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

/// Request to create a new automation
#[derive(Debug, Clone, Deserialize)]
pub struct CreateAutomationRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub trigger: Trigger,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    pub actions: Vec<Action>,
}

fn default_enabled() -> bool {
    true
}

/// Request to update an automation
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAutomationRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<Option<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub trigger: Option<Trigger>,
    #[serde(default)]
    pub conditions: Option<Vec<Condition>>,
    #[serde(default)]
    pub actions: Option<Vec<Action>>,
}

impl Automation {
    /// Create a new automation from a create request
    pub fn from_request(request: CreateAutomationRequest) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: request.name,
            description: request.description,
            enabled: request.enabled,
            trigger: request.trigger,
            conditions: request.conditions,
            actions: request.actions,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Apply an update request to this automation
    pub fn apply_update(&mut self, update: UpdateAutomationRequest) {
        if let Some(name) = update.name {
            self.name = name;
        }
        if let Some(description) = update.description {
            self.description = description;
        }
        if let Some(enabled) = update.enabled {
            self.enabled = enabled;
        }
        if let Some(trigger) = update.trigger {
            self.trigger = trigger;
        }
        if let Some(conditions) = update.conditions {
            self.conditions = conditions;
        }
        if let Some(actions) = update.actions {
            self.actions = actions;
        }
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
}
