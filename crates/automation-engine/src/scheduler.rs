//! Scheduler for time-based automation triggers

use crate::error::AutomationError;
use crate::model::{Automation, ScheduleSpec, Trigger};
use chrono::{Datelike, Local, NaiveTime};
use cron::Schedule;
use dashmap::DashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

/// Events emitted by the scheduler
#[derive(Debug, Clone)]
pub struct SchedulerEvent {
    pub automation_id: String,
}

/// Scheduler for managing time-based automation triggers
pub struct Scheduler {
    /// Active timer handles (keyed by automation ID)
    timers: Arc<DashMap<String, JoinHandle<()>>>,
    /// Event sender for scheduled triggers
    event_tx: broadcast::Sender<SchedulerEvent>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    /// Create a new scheduler
    #[must_use] pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            timers: Arc::new(DashMap::new()),
            event_tx,
        }
    }

    /// Subscribe to scheduler events
    #[must_use] pub fn subscribe(&self) -> broadcast::Receiver<SchedulerEvent> {
        self.event_tx.subscribe()
    }

    /// Register an automation with a schedule trigger
    pub fn register(&self, automation: &Automation) -> Result<(), AutomationError> {
        // Only handle schedule triggers
        let schedule = match &automation.trigger {
            Trigger::Schedule { schedule } => schedule,
            _ => return Ok(()), // Not a schedule trigger, nothing to do
        };

        if !automation.enabled {
            // Remove any existing timer for disabled automations
            self.remove(&automation.id);
            return Ok(());
        }

        // Remove existing timer if present
        self.remove(&automation.id);

        // Create new timer based on schedule type
        match schedule {
            ScheduleSpec::Interval { seconds } => {
                self.schedule_interval(&automation.id, *seconds);
            }
            ScheduleSpec::TimeOfDay { time, days } => {
                self.schedule_time_of_day(&automation.id, time, days)?;
            }
            ScheduleSpec::Cron { expression } => {
                self.schedule_cron(&automation.id, expression)?;
            }
        }

        Ok(())
    }

    /// Remove an automation from the scheduler
    pub fn remove(&self, automation_id: &str) {
        if let Some((_, handle)) = self.timers.remove(automation_id) {
            handle.abort();
            tracing::debug!("Removed scheduler timer for automation {}", automation_id);
        }
    }

    /// Update an automation's schedule
    pub fn update(&self, automation: &Automation) -> Result<(), AutomationError> {
        // Re-register (which handles removal and recreation)
        self.register(automation)
    }

    /// Schedule an interval-based trigger
    fn schedule_interval(&self, automation_id: &str, seconds: u64) {
        let id = automation_id.to_string();
        let event_tx = self.event_tx.clone();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(seconds));
            // Skip the first immediate tick
            interval.tick().await;

            loop {
                interval.tick().await;
                tracing::debug!("Interval trigger fired for automation {}", id);
                let _ = event_tx.send(SchedulerEvent {
                    automation_id: id.clone(),
                });
            }
        });

        self.timers.insert(automation_id.to_string(), handle);
        tracing::info!(
            "Scheduled interval trigger every {}s for automation {}",
            seconds,
            automation_id
        );
    }

    /// Schedule a time-of-day trigger
    fn schedule_time_of_day(
        &self,
        automation_id: &str,
        time_str: &str,
        days: &[u8],
    ) -> Result<(), AutomationError> {
        let target_time = NaiveTime::parse_from_str(time_str, "%H:%M")
            .map_err(|_| AutomationError::InvalidTimeFormat(time_str.to_string()))?;

        let id = automation_id.to_string();
        let event_tx = self.event_tx.clone();
        let days_filter = days.to_vec();
        let days_log = days.to_vec();

        let handle = tokio::spawn(async move {
            loop {
                // Calculate time until next trigger
                let now = Local::now();
                let today = now.date_naive();
                let mut target_datetime = today.and_time(target_time);

                // If we've passed today's time, move to tomorrow
                if target_datetime <= now.naive_local() {
                    target_datetime += chrono::Duration::days(1);
                }

                // Check day-of-week filter
                if !days_filter.is_empty() {
                    let mut dt = target_datetime;
                    let mut attempts = 0;
                    while !days_filter.contains(&(dt.weekday().num_days_from_sunday() as u8))
                        && attempts < 7
                    {
                        dt += chrono::Duration::days(1);
                        attempts += 1;
                    }
                    target_datetime = dt;
                }

                // Calculate sleep duration
                let target_instant = target_datetime.and_local_timezone(Local).unwrap();
                let duration = (target_instant - now)
                    .to_std()
                    .unwrap_or(std::time::Duration::from_secs(1));

                tracing::debug!(
                    "Next time-of-day trigger for {} at {} (in {:?})",
                    id,
                    target_datetime,
                    duration
                );

                tokio::time::sleep(duration).await;

                tracing::debug!("Time-of-day trigger fired for automation {}", id);
                let _ = event_tx.send(SchedulerEvent {
                    automation_id: id.clone(),
                });

                // Small delay to avoid double-firing
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });

        self.timers.insert(automation_id.to_string(), handle);
        tracing::info!(
            "Scheduled time-of-day trigger at {} (days: {:?}) for automation {}",
            time_str,
            days_log,
            automation_id
        );
        Ok(())
    }

    /// Schedule a cron-based trigger
    fn schedule_cron(&self, automation_id: &str, expression: &str) -> Result<(), AutomationError> {
        let schedule = Schedule::from_str(expression)
            .map_err(|e| AutomationError::InvalidCron(format!("{expression}: {e}")))?;

        let id = automation_id.to_string();
        let event_tx = self.event_tx.clone();

        let handle = tokio::spawn(async move {
            loop {
                // Find next scheduled time
                let now = Local::now();
                let next = schedule.upcoming(Local).next();

                let Some(next_time) = next else {
                    tracing::warn!("No upcoming times for cron schedule {}", id);
                    break;
                };

                let duration = (next_time - now)
                    .to_std()
                    .unwrap_or(std::time::Duration::from_secs(60));

                tracing::debug!(
                    "Next cron trigger for {} at {} (in {:?})",
                    id,
                    next_time,
                    duration
                );

                tokio::time::sleep(duration).await;

                tracing::debug!("Cron trigger fired for automation {}", id);
                let _ = event_tx.send(SchedulerEvent {
                    automation_id: id.clone(),
                });

                // Small delay to avoid double-firing
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });

        self.timers.insert(automation_id.to_string(), handle);
        tracing::info!(
            "Scheduled cron trigger '{}' for automation {}",
            expression,
            automation_id
        );
        Ok(())
    }

    /// Get the number of active timers
    #[must_use] pub fn active_count(&self) -> usize {
        self.timers.len()
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        // Abort all timer tasks
        for entry in self.timers.iter() {
            entry.value().abort();
        }
    }
}
