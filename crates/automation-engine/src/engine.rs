//! Core automation engine

use crate::error::AutomationError;
use crate::evaluator::ConditionEvaluator;
use crate::executor::ActionExecutor;
use crate::model::{
    Automation, CreateAutomationRequest, StateChange, Trigger, UpdateAutomationRequest,
};
use crate::persistence;
use crate::scheduler::Scheduler;
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use zigbee_core::{network::NetworkEvent, ZigbeeNetwork};

/// Events emitted by the automation engine
#[derive(Debug, Clone)]
pub enum AutomationEvent {
    /// An automation was triggered
    Triggered {
        automation_id: String,
        trigger_reason: String,
    },
    /// An automation action was executed
    ActionExecuted {
        automation_id: String,
        action_index: usize,
    },
    /// An automation failed
    Failed {
        automation_id: String,
        error: String,
    },
    /// An automation was created
    Created { automation_id: String },
    /// An automation was updated
    Updated { automation_id: String },
    /// An automation was deleted
    Deleted { automation_id: String },
}

/// The main automation engine
pub struct AutomationEngine {
    /// All registered automations
    automations: Arc<DashMap<String, Automation>>,
    /// Reference to zigbee network for device control
    network: Option<Arc<ZigbeeNetwork>>,
    /// Condition evaluator
    evaluator: Arc<ConditionEvaluator>,
    /// Action executor
    executor: Arc<ActionExecutor>,
    /// Time-based scheduler
    scheduler: Arc<Scheduler>,
    /// Event broadcaster
    event_tx: broadcast::Sender<AutomationEvent>,
    /// Path for persistence
    data_path: PathBuf,
}

impl AutomationEngine {
    /// Create a new automation engine
    pub async fn new(
        network: Option<Arc<ZigbeeNetwork>>,
        data_dir: &std::path::Path,
    ) -> Result<Self, AutomationError> {
        let (event_tx, _) = broadcast::channel(64);
        let data_path = data_dir.join("automations.json");

        let evaluator = Arc::new(ConditionEvaluator::new(network.clone()));
        let executor = Arc::new(ActionExecutor::new(network.clone()));
        let scheduler = Arc::new(Scheduler::new());

        let engine = Self {
            automations: Arc::new(DashMap::new()),
            network,
            evaluator,
            executor,
            scheduler,
            event_tx,
            data_path,
        };

        // Load persisted automations
        engine.load().await?;

        Ok(engine)
    }

    /// Load automations from disk
    async fn load(&self) -> Result<(), AutomationError> {
        let automations = persistence::load_automations(&self.data_path).await;
        for automation in automations {
            // Register with scheduler if needed
            if let Err(e) = self.scheduler.register(&automation) {
                tracing::warn!("Failed to schedule automation {}: {}", automation.id, e);
            }
            self.automations.insert(automation.id.clone(), automation);
        }
        Ok(())
    }

    /// Save automations to disk
    async fn save(&self) -> Result<(), AutomationError> {
        let automations: Vec<Automation> =
            self.automations.iter().map(|r| r.value().clone()).collect();
        persistence::save_automations(&self.data_path, &automations).await?;
        Ok(())
    }

    /// Start the engine (subscribe to events, start scheduler)
    pub fn start(self: &Arc<Self>) {
        // Start device event listener if we have a network
        if let Some(network) = &self.network {
            self.start_device_listener(network.clone());
        }

        // Start scheduler event listener
        self.start_scheduler_listener();
    }

    /// Subscribe to automation events
    pub fn subscribe(&self) -> broadcast::Receiver<AutomationEvent> {
        self.event_tx.subscribe()
    }

    /// Get all automations
    pub fn list(&self) -> Vec<Automation> {
        self.automations.iter().map(|r| r.value().clone()).collect()
    }

    /// Get automation by ID
    pub fn get(&self, id: &str) -> Option<Automation> {
        self.automations.get(id).map(|r| r.value().clone())
    }

    /// Create a new automation
    pub async fn create(
        &self,
        request: CreateAutomationRequest,
    ) -> Result<Automation, AutomationError> {
        let automation = Automation::from_request(request);

        // Register with scheduler if needed
        self.scheduler.register(&automation)?;

        self.automations
            .insert(automation.id.clone(), automation.clone());
        self.save().await?;

        let _ = self.event_tx.send(AutomationEvent::Created {
            automation_id: automation.id.clone(),
        });

        tracing::info!(
            "Created automation: {} ({})",
            automation.name,
            automation.id
        );
        Ok(automation)
    }

    /// Update an automation
    pub async fn update(
        &self,
        id: &str,
        request: UpdateAutomationRequest,
    ) -> Result<Automation, AutomationError> {
        let mut automation = self
            .automations
            .get_mut(id)
            .ok_or_else(|| AutomationError::NotFound(id.to_string()))?;

        automation.apply_update(request);

        // Update scheduler
        self.scheduler.update(&automation)?;

        let updated = automation.clone();
        drop(automation);

        self.save().await?;

        let _ = self.event_tx.send(AutomationEvent::Updated {
            automation_id: id.to_string(),
        });

        tracing::info!("Updated automation: {}", id);
        Ok(updated)
    }

    /// Delete an automation
    pub async fn delete(&self, id: &str) -> Result<Automation, AutomationError> {
        let (_, automation) = self
            .automations
            .remove(id)
            .ok_or_else(|| AutomationError::NotFound(id.to_string()))?;

        self.scheduler.remove(id);
        self.save().await?;

        let _ = self.event_tx.send(AutomationEvent::Deleted {
            automation_id: id.to_string(),
        });

        tracing::info!("Deleted automation: {} ({})", automation.name, id);
        Ok(automation)
    }

    /// Enable an automation
    pub async fn enable(&self, id: &str) -> Result<Automation, AutomationError> {
        self.update(
            id,
            UpdateAutomationRequest {
                enabled: Some(true),
                ..Default::default()
            },
        )
        .await
    }

    /// Disable an automation
    pub async fn disable(&self, id: &str) -> Result<Automation, AutomationError> {
        self.update(
            id,
            UpdateAutomationRequest {
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
    }

    /// Manually trigger an automation
    pub async fn trigger(&self, id: &str) -> Result<(), AutomationError> {
        let automation = self
            .automations
            .get(id)
            .ok_or_else(|| AutomationError::NotFound(id.to_string()))?
            .clone();

        if !automation.enabled {
            return Err(AutomationError::Disabled(id.to_string()));
        }

        self.execute_automation(&automation, "manual").await
    }

    /// Execute an automation
    async fn execute_automation(
        &self,
        automation: &Automation,
        trigger_reason: &str,
    ) -> Result<(), AutomationError> {
        tracing::info!(
            "Executing automation '{}' (trigger: {})",
            automation.name,
            trigger_reason
        );

        let _ = self.event_tx.send(AutomationEvent::Triggered {
            automation_id: automation.id.clone(),
            trigger_reason: trigger_reason.to_string(),
        });

        // Evaluate conditions
        if !self.evaluator.evaluate_all(&automation.conditions)? {
            tracing::debug!(
                "Automation '{}' conditions not met, skipping",
                automation.name
            );
            return Ok(());
        }

        // Execute actions
        let result = self
            .executor
            .execute_actions(&automation.id, &automation.actions)
            .await;

        if let Err(ref e) = result {
            let _ = self.event_tx.send(AutomationEvent::Failed {
                automation_id: automation.id.clone(),
                error: e.to_string(),
            });
        }

        result
    }

    /// Start listening for device events
    fn start_device_listener(self: &Arc<Self>, network: Arc<ZigbeeNetwork>) {
        let engine = Arc::clone(self);
        let mut rx = network.subscribe();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        engine.handle_network_event(event).await;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Automation engine lagged by {} events", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("Network event channel closed");
                        break;
                    }
                }
            }
        });
    }

    /// Handle a network event
    async fn handle_network_event(&self, event: NetworkEvent) {
        for entry in self.automations.iter() {
            let automation = entry.value();
            if !automation.enabled {
                continue;
            }

            if self.trigger_matches(&automation.trigger, &event) {
                if let Err(e) = self.execute_automation(automation, "device_state").await {
                    tracing::error!("Failed to execute automation '{}': {}", automation.name, e);
                }
            }
        }
    }

    /// Check if a trigger matches a network event
    fn trigger_matches(&self, trigger: &Trigger, event: &NetworkEvent) -> bool {
        match trigger {
            Trigger::DeviceState {
                device_ieee,
                endpoint: trigger_endpoint,
                state_change,
            } => match event {
                NetworkEvent::DeviceJoined(device) => {
                    let ieee_str = format_ieee(&device.ieee_address);
                    matches!(state_change, StateChange::Joined | StateChange::Any)
                        && ieee_str == *device_ieee
                }
                NetworkEvent::DeviceLeft { ieee_address } => {
                    let ieee_str = format_ieee(ieee_address);
                    matches!(state_change, StateChange::Left | StateChange::Any)
                        && ieee_str == *device_ieee
                }
                NetworkEvent::DeviceUpdated { ieee_address } => {
                    let ieee_str = format_ieee(ieee_address);
                    matches!(state_change, StateChange::Any) && ieee_str == *device_ieee
                }
                NetworkEvent::NetworkStateChanged { .. } => false,
                NetworkEvent::DeviceStateChanged {
                    ieee_address,
                    endpoint,
                    state_on,
                } => {
                    let ieee_str = format_ieee(ieee_address);
                    if ieee_str != *device_ieee {
                        return false;
                    }
                    // Check endpoint filter if specified
                    if let Some(ep) = trigger_endpoint {
                        if *ep != *endpoint {
                            return false;
                        }
                    }
                    // Match state change type
                    match state_change {
                        StateChange::Any | StateChange::Toggled => true,
                        StateChange::TurnedOn => *state_on,
                        StateChange::TurnedOff => !*state_on,
                        _ => false,
                    }
                }
            },
            _ => false, // Schedule and Manual triggers are handled separately
        }
    }

    /// Start listening for scheduler events
    fn start_scheduler_listener(self: &Arc<Self>) {
        let engine = Arc::clone(self);
        let mut rx = self.scheduler.subscribe();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let Some(automation) = engine.get(&event.automation_id) {
                            if automation.enabled {
                                if let Err(e) =
                                    engine.execute_automation(&automation, "schedule").await
                                {
                                    tracing::error!(
                                        "Failed to execute scheduled automation '{}': {}",
                                        automation.name,
                                        e
                                    );
                                }
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Scheduler listener lagged by {} events", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("Scheduler event channel closed");
                        break;
                    }
                }
            }
        });
    }
}

/// Format IEEE address as string (e.g., "00:11:22:33:44:55:66:77")
fn format_ieee(ieee: &[u8; 8]) -> String {
    ieee.iter()
        .rev()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

impl Default for UpdateAutomationRequest {
    fn default() -> Self {
        Self {
            name: None,
            description: None,
            enabled: None,
            trigger: None,
            conditions: None,
            actions: None,
        }
    }
}
