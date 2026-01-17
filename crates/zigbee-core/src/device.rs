//! Zigbee device representation

use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Zigbee device types (network role)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Coordinator,
    Router,
    EndDevice,
}

/// Device category for user classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceCategory {
    Light,
    Outlet,
    Switch,
    Sensor,
    Lock,
    Thermostat,
    Fan,
    Blinds,
    Other,
}

impl Default for DeviceCategory {
    fn default() -> Self {
        Self::Other
    }
}

/// A Zigbee device on the network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZigbeeDevice {
    /// IEEE address (EUI-64)
    pub ieee_address: [u8; 8],
    /// Network short address
    pub nwk_address: u16,
    /// Device type (network role)
    pub device_type: DeviceType,
    /// User-assigned device category
    #[serde(default)]
    pub category: DeviceCategory,
    /// Manufacturer name (from Basic cluster)
    pub manufacturer: Option<String>,
    /// Model identifier (from Basic cluster)
    pub model: Option<String>,
    /// User-assigned friendly name
    pub friendly_name: Option<String>,
    /// Device endpoints
    pub endpoints: Vec<Endpoint>,
    /// Last seen timestamp
    #[serde(skip)]
    pub last_seen: Option<Instant>,
    /// Link quality indicator (0-255)
    pub lqi: Option<u8>,
    /// Is device reachable
    pub available: bool,
    /// Current on/off state (if applicable)
    #[serde(default)]
    pub state_on: Option<bool>,
}

impl ZigbeeDevice {
    /// Create a new device with just address info
    #[must_use] pub fn new(ieee_address: [u8; 8], nwk_address: u16) -> Self {
        Self {
            ieee_address,
            nwk_address,
            device_type: DeviceType::EndDevice,
            category: DeviceCategory::default(),
            manufacturer: None,
            model: None,
            friendly_name: None,
            endpoints: Vec::new(),
            last_seen: None,
            lqi: None,
            available: true,
            state_on: None,
        }
    }

    /// Get IEEE address as hex string
    #[must_use] pub fn ieee_address_string(&self) -> String {
        self.ieee_address
            .iter()
            .rev() // IEEE addresses are typically displayed in reverse byte order
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":")
    }

    /// Get a display name (friendly name, model, or IEEE address)
    #[must_use] pub fn display_name(&self) -> String {
        self.friendly_name
            .clone()
            .or_else(|| self.model.clone())
            .unwrap_or_else(|| self.ieee_address_string())
    }
}

/// A device endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    /// Endpoint ID (1-240)
    pub id: u8,
    /// Profile ID (e.g., 0x0104 for Home Automation)
    pub profile_id: u16,
    /// Device ID within the profile
    pub device_id: u16,
    /// Input (server) clusters
    pub in_clusters: Vec<u16>,
    /// Output (client) clusters
    pub out_clusters: Vec<u16>,
}

impl Endpoint {
    /// Check if endpoint has a specific cluster
    #[must_use] pub fn has_cluster(&self, cluster_id: u16) -> bool {
        self.in_clusters.contains(&cluster_id) || self.out_clusters.contains(&cluster_id)
    }

    /// Check if this is a light endpoint
    #[must_use] pub fn is_light(&self) -> bool {
        // Check for On/Off cluster (0x0006) or Level Control (0x0008)
        self.has_cluster(0x0006) || self.has_cluster(0x0008)
    }

    /// Check if this is a color light endpoint
    #[must_use] pub fn is_color_light(&self) -> bool {
        self.has_cluster(0x0300) // Color Control cluster
    }

    /// Check if this has temperature sensor
    #[must_use] pub fn has_temperature(&self) -> bool {
        self.has_cluster(0x0402)
    }

    /// Check if this has humidity sensor
    #[must_use] pub fn has_humidity(&self) -> bool {
        self.has_cluster(0x0405)
    }

    /// Check if this is an occupancy sensor
    #[must_use] pub fn is_occupancy_sensor(&self) -> bool {
        self.has_cluster(0x0406)
    }
}
