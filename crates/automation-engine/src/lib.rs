//! Automation engine for Casita Assistant
//!
//! Provides rule-based automation with triggers, conditions, and actions
//! for controlling smart home devices.

pub mod engine;
pub mod error;
pub mod evaluator;
pub mod executor;
pub mod model;
pub mod persistence;
pub mod scheduler;

pub use engine::{AutomationEngine, AutomationEvent};
pub use error::AutomationError;
pub use model::*;
