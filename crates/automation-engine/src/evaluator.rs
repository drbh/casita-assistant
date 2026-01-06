//! Condition evaluator for automations

use crate::error::AutomationError;
use crate::model::Condition;
use chrono::{Datelike, Local, NaiveTime};
use std::sync::Arc;
use zigbee_core::ZigbeeNetwork;

/// Evaluator for automation conditions
pub struct ConditionEvaluator {
    network: Option<Arc<ZigbeeNetwork>>,
}

impl ConditionEvaluator {
    /// Create a new condition evaluator
    pub fn new(network: Option<Arc<ZigbeeNetwork>>) -> Self {
        Self { network }
    }

    /// Evaluate all conditions (all must pass for AND semantics)
    pub fn evaluate_all(&self, conditions: &[Condition]) -> Result<bool, AutomationError> {
        for condition in conditions {
            if !self.evaluate(condition)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Evaluate a single condition
    pub fn evaluate(&self, condition: &Condition) -> Result<bool, AutomationError> {
        match condition {
            Condition::TimeRange { start, end } => self.evaluate_time_range(start, end),
            Condition::DayOfWeek { days } => self.evaluate_day_of_week(days),
            Condition::DeviceAvailable {
                device_ieee,
                available,
            } => self.evaluate_device_available(device_ieee, *available),
            Condition::And { conditions } => {
                for c in conditions {
                    if !self.evaluate(c)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Condition::Or { conditions } => {
                for c in conditions {
                    if self.evaluate(c)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Condition::Not { condition } => Ok(!self.evaluate(condition)?),
        }
    }

    /// Evaluate time range condition
    fn evaluate_time_range(&self, start: &str, end: &str) -> Result<bool, AutomationError> {
        let start_time = parse_time(start)?;
        let end_time = parse_time(end)?;
        let now = Local::now().time();

        // Handle wrap-around (e.g., 22:00 to 06:00)
        let in_range = if start_time <= end_time {
            // Normal range (e.g., 09:00 to 17:00)
            now >= start_time && now <= end_time
        } else {
            // Wrap-around range (e.g., 22:00 to 06:00)
            now >= start_time || now <= end_time
        };

        Ok(in_range)
    }

    /// Evaluate day of week condition
    fn evaluate_day_of_week(&self, days: &[u8]) -> Result<bool, AutomationError> {
        if days.is_empty() {
            return Ok(true); // Empty means every day
        }

        let today = Local::now().weekday().num_days_from_sunday() as u8;
        Ok(days.contains(&today))
    }

    /// Evaluate device availability condition
    fn evaluate_device_available(
        &self,
        device_ieee: &str,
        should_be_available: bool,
    ) -> Result<bool, AutomationError> {
        let Some(network) = &self.network else {
            // No network, can't check device availability
            return Ok(false);
        };

        let ieee = parse_ieee_address(device_ieee)?;
        let is_available = network
            .get_device(&ieee)
            .map(|d| d.available)
            .unwrap_or(false);

        Ok(is_available == should_be_available)
    }
}

/// Parse a time string in HH:MM format
fn parse_time(s: &str) -> Result<NaiveTime, AutomationError> {
    NaiveTime::parse_from_str(s, "%H:%M")
        .map_err(|_| AutomationError::InvalidTimeFormat(s.to_string()))
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

    // Reverse to match internal representation
    let mut arr = [0u8; 8];
    for (i, &b) in bytes.iter().rev().enumerate() {
        arr[i] = b;
    }
    Ok(arr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ieee_address() {
        let result = parse_ieee_address("00:11:22:33:44:55:66:77").unwrap();
        assert_eq!(result, [0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00]);
    }

    #[test]
    fn test_day_of_week_empty() {
        let evaluator = ConditionEvaluator::new(None);
        assert!(evaluator.evaluate_day_of_week(&[]).unwrap());
    }
}
