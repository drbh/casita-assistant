//! Zigbee network management

use crate::device::{DeviceCategory, DeviceType, ZigbeeDevice};
use crate::persistence;
use dashmap::DashMap;
use deconz_protocol::{
    clusters, profiles, ActiveEndpointsResponse, ApsDataIndication, ApsDataRequest, DeconzEvent,
    DeconzTransport, NetworkParameter, OnOffCommand, SimpleDescriptorResponse, ZclFrame,
    ZdoCluster,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tokio::sync::broadcast;

/// Network errors
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Protocol error: {0}")]
    Protocol(#[from] deconz_protocol::ProtocolError),

    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Network not connected")]
    NotConnected,
}

/// Network events
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// A new device joined the network
    DeviceJoined(ZigbeeDevice),
    /// A device left the network
    DeviceLeft { ieee_address: [u8; 8] },
    /// Device state/attributes updated
    DeviceUpdated { ieee_address: [u8; 8] },
    /// Network state changed
    NetworkStateChanged { connected: bool },
    /// Device on/off state changed
    DeviceStateChanged {
        ieee_address: [u8; 8],
        endpoint: u8,
        state_on: bool,
    },
}

/// Network status information
#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkStatus {
    pub connected: bool,
    pub channel: u8,
    pub pan_id: u16,
    pub extended_pan_id: String,
    pub permit_join: bool,
    pub device_count: usize,
}

/// Zigbee network manager
pub struct ZigbeeNetwork {
    /// Low-level transport
    transport: Arc<DeconzTransport>,
    /// Known devices (keyed by IEEE address)
    devices: Arc<DashMap<[u8; 8], ZigbeeDevice>>,
    /// Event broadcaster
    event_tx: broadcast::Sender<NetworkEvent>,
    /// Path to device data file for persistence
    data_path: Option<PathBuf>,
}

impl ZigbeeNetwork {
    /// Create a new network manager
    pub async fn new(serial_path: &str) -> Result<Self, NetworkError> {
        // Determine data directory from env or use default
        let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string());
        let data_path = PathBuf::from(data_dir).join("devices.json");

        let transport = Arc::new(DeconzTransport::connect(serial_path).await?);

        let (event_tx, _) = broadcast::channel(64);

        // Load persisted devices
        let devices = Arc::new(DashMap::new());
        let loaded = persistence::load_devices(&data_path).await;
        for device in loaded {
            devices.insert(device.ieee_address, device);
        }

        let network = Self {
            transport: transport.clone(),
            devices,
            event_tx,
            data_path: Some(data_path),
        };

        // Start background task to listen for device events
        network.start_event_listener(transport);

        Ok(network)
    }

    /// Start background task to listen for deCONZ events
    fn start_event_listener(&self, transport: Arc<DeconzTransport>) {
        let devices = Arc::clone(&self.devices);
        let event_tx = self.event_tx.clone();
        let mut deconz_rx = transport.subscribe();
        let transport_clone = transport.clone();
        let data_path = self.data_path.clone();

        tokio::spawn(async move {
            loop {
                match deconz_rx.recv().await {
                    Ok(DeconzEvent::ApsDataAvailable) => {
                        // Automatically fetch APS data when available
                        tracing::debug!("APS data available, fetching...");
                        if let Err(e) = transport_clone.request_aps_data().await {
                            tracing::warn!("Failed to fetch APS data: {}", e);
                        }
                    }
                    Ok(DeconzEvent::DeviceStateChanged(state)) => {
                        // If aps_data_indication is set, fetch the data
                        if state.aps_data_indication {
                            tracing::debug!(
                                "Device state indicates APS data available, fetching..."
                            );
                            if let Err(e) = transport_clone.request_aps_data().await {
                                tracing::warn!("Failed to fetch APS data: {}", e);
                            }
                        }
                    }
                    Ok(DeconzEvent::DeviceAnnounced {
                        ieee_addr,
                        short_addr,
                        capability,
                    }) => {
                        let ieee_str = ApsDataIndication::format_ieee(&ieee_addr);
                        tracing::info!(
                            "Registering device: IEEE={} short={:#06x}",
                            ieee_str,
                            short_addr
                        );

                        // Determine device type from capability byte
                        let device_type = if (capability & 0x02) != 0 {
                            DeviceType::Router
                        } else {
                            DeviceType::EndDevice
                        };

                        let is_new = !devices.contains_key(&ieee_addr);

                        // Create or update device
                        let device = if let Some(mut existing) = devices.get_mut(&ieee_addr) {
                            existing.nwk_address = short_addr;
                            existing.last_seen = Some(Instant::now());
                            existing.available = true;
                            existing.clone()
                        } else {
                            let mut new_device = ZigbeeDevice::new(ieee_addr, short_addr);
                            new_device.device_type = device_type;
                            new_device.last_seen = Some(Instant::now());
                            devices.insert(ieee_addr, new_device.clone());
                            new_device
                        };

                        // Emit network event
                        let event = if is_new {
                            NetworkEvent::DeviceJoined(device)
                        } else {
                            NetworkEvent::DeviceUpdated {
                                ieee_address: ieee_addr,
                            }
                        };
                        let _ = event_tx.send(event);

                        // Persist device changes
                        if let Some(ref path) = data_path {
                            let devices_vec: Vec<ZigbeeDevice> =
                                devices.iter().map(|r| r.value().clone()).collect();
                            let path = path.clone();
                            tokio::spawn(async move {
                                if let Err(e) = persistence::save_devices(&path, &devices_vec).await
                                {
                                    tracing::warn!("Failed to save devices: {}", e);
                                }
                            });
                        }

                        // Auto-discover endpoints for new devices
                        if is_new {
                            let req = ApsDataRequest::active_endpoints_request(1, short_addr, 1);
                            let tc = transport_clone.clone();
                            tokio::spawn(async move {
                                // Small delay to let device settle
                                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                if let Err(e) = tc.send_aps_request(req).await {
                                    tracing::warn!("Failed to request active endpoints: {}", e);
                                }
                            });
                        }
                    }
                    Ok(DeconzEvent::MacPoll { short_addr }) => {
                        // Update last_seen for device with this short address
                        for mut entry in devices.iter_mut() {
                            if entry.nwk_address == short_addr {
                                entry.last_seen = Some(Instant::now());
                                entry.available = true;
                                break;
                            }
                        }
                    }
                    Ok(DeconzEvent::ApsIndication(indication)) => {
                        // Handle Home Automation profile (button presses, device commands)
                        if indication.profile_id == profiles::HOME_AUTOMATION {
                            // Parse ZCL frame from ASDU
                            if let Ok(zcl) = ZclFrame::parse(&indication.asdu) {
                                // Handle On/Off cluster commands
                                if indication.cluster_id == clusters::ON_OFF
                                    && zcl.is_cluster_specific()
                                {
                                    let cmd_id = zcl.command_id();
                                    let state_on = match cmd_id {
                                        0x00 => Some(false), // Off
                                        0x01 => Some(true),  // On
                                        0x02 => None,        // Toggle (will resolve to opposite)
                                        _ => {
                                            tracing::debug!(
                                                "Unknown On/Off command: {:#04x}",
                                                cmd_id
                                            );
                                            continue;
                                        }
                                    };

                                    // Find the source device by short address
                                    let mut found_device = None;
                                    for entry in devices.iter() {
                                        if entry.nwk_address == indication.src_short_addr {
                                            found_device =
                                                Some((entry.ieee_address, indication.src_endpoint));
                                            break;
                                        }
                                    }

                                    if let Some((ieee_address, endpoint)) = found_device {
                                        let resolved_state = match state_on {
                                            Some(s) => s,
                                            None => {
                                                // Toggle: get current state and flip it
                                                devices
                                                    .get(&ieee_address)
                                                    .map(|d| !d.state_on.unwrap_or(false))
                                                    .unwrap_or(true)
                                            }
                                        };

                                        tracing::info!(
                                            "Device {:#04x} sent On/Off command: {} (state={})",
                                            indication.src_short_addr,
                                            match cmd_id {
                                                0x00 => "Off",
                                                0x01 => "On",
                                                0x02 => "Toggle",
                                                _ => "?",
                                            },
                                            resolved_state
                                        );

                                        // Emit event for automation engine
                                        let _ = event_tx.send(NetworkEvent::DeviceStateChanged {
                                            ieee_address,
                                            endpoint,
                                            state_on: resolved_state,
                                        });
                                    } else {
                                        tracing::debug!(
                                            "On/Off command from unknown device {:#06x}",
                                            indication.src_short_addr
                                        );
                                    }
                                }
                            }
                        }
                        // Handle ZDO responses
                        else if indication.profile_id == profiles::ZDO {
                            match indication.cluster_id {
                                x if x == ZdoCluster::ActiveEpRsp as u16 => {
                                    if let Ok(resp) =
                                        ActiveEndpointsResponse::parse(&indication.asdu)
                                    {
                                        if resp.status == 0 {
                                            tracing::info!(
                                                "Active endpoints for {:#06x}: {:?}",
                                                resp.nwk_addr,
                                                resp.endpoints
                                            );
                                            // Request simple descriptor for each endpoint
                                            for ep in &resp.endpoints {
                                                let req = ApsDataRequest::simple_descriptor_request(
                                                    1,
                                                    resp.nwk_addr,
                                                    *ep,
                                                    1,
                                                );
                                                let tc = transport_clone.clone();
                                                tokio::spawn(async move {
                                                    if let Err(e) = tc.send_aps_request(req).await {
                                                        tracing::warn!("Failed to request simple descriptor: {}", e);
                                                    }
                                                });
                                            }
                                        }
                                    }
                                }
                                x if x == ZdoCluster::SimpleDescRsp as u16 => {
                                    if let Ok(resp) =
                                        SimpleDescriptorResponse::parse(&indication.asdu)
                                    {
                                        if resp.status == 0 {
                                            tracing::info!(
                                                "Simple descriptor for {:#06x} EP{}: profile={:#06x} device={:#06x} in={:04x?} out={:04x?}",
                                                resp.nwk_addr,
                                                resp.endpoint,
                                                resp.profile_id,
                                                resp.device_id,
                                                resp.in_clusters,
                                                resp.out_clusters
                                            );
                                            // Update device with endpoint info
                                            for mut entry in devices.iter_mut() {
                                                if entry.nwk_address == resp.nwk_addr {
                                                    let ep = crate::device::Endpoint {
                                                        id: resp.endpoint,
                                                        profile_id: resp.profile_id,
                                                        device_id: resp.device_id,
                                                        in_clusters: resp.in_clusters.clone(),
                                                        out_clusters: resp.out_clusters.clone(),
                                                    };
                                                    // Add or update endpoint
                                                    if let Some(existing) = entry
                                                        .endpoints
                                                        .iter_mut()
                                                        .find(|e| e.id == resp.endpoint)
                                                    {
                                                        *existing = ep;
                                                    } else {
                                                        entry.endpoints.push(ep);
                                                    }
                                                    let _ = event_tx.send(
                                                        NetworkEvent::DeviceUpdated {
                                                            ieee_address: entry.ieee_address,
                                                        },
                                                    );
                                                    // Persist
                                                    if let Some(ref path) = data_path {
                                                        let devices_vec: Vec<ZigbeeDevice> =
                                                            devices
                                                                .iter()
                                                                .map(|r| r.value().clone())
                                                                .collect();
                                                        let path = path.clone();
                                                        tokio::spawn(async move {
                                                            if let Err(e) =
                                                                persistence::save_devices(
                                                                    &path,
                                                                    &devices_vec,
                                                                )
                                                                .await
                                                            {
                                                                tracing::warn!(
                                                                    "Failed to save devices: {}",
                                                                    e
                                                                );
                                                            }
                                                        });
                                                    }
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Ok(_) => {} // Ignore other events
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Event listener lagged by {} events", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("Event channel closed, stopping listener");
                        break;
                    }
                }
            }
        });
    }

    /// Get the underlying transport
    pub fn transport(&self) -> &DeconzTransport {
        &self.transport
    }

    /// Subscribe to network events
    pub fn subscribe(&self) -> broadcast::Receiver<NetworkEvent> {
        self.event_tx.subscribe()
    }

    /// Get network status
    pub async fn get_status(&self) -> Result<NetworkStatus, NetworkError> {
        let state = self.transport.get_device_state().await?;

        // Read network parameters
        let channel = self
            .transport
            .read_parameter(NetworkParameter::CurrentChannel)
            .await
            .map(|v| v.first().copied().unwrap_or(0))
            .unwrap_or(0);

        let pan_id = self
            .transport
            .read_parameter(NetworkParameter::NwkPanId)
            .await
            .map(|v| {
                if v.len() >= 2 {
                    u16::from_le_bytes([v[0], v[1]])
                } else {
                    0
                }
            })
            .unwrap_or(0);

        let extended_pan_id = self
            .transport
            .read_parameter(NetworkParameter::NwkExtendedPanId)
            .await
            .map(|v| {
                v.iter()
                    .rev()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(":")
            })
            .unwrap_or_else(|_| "unknown".to_string());

        let permit_join = self
            .transport
            .read_parameter(NetworkParameter::PermitJoin)
            .await
            .map(|v| v.first().copied().unwrap_or(0) > 0)
            .unwrap_or(false);

        Ok(NetworkStatus {
            connected: state.network_state == deconz_protocol::NetworkState::Connected,
            channel,
            pan_id,
            extended_pan_id,
            permit_join,
            device_count: self.devices.len(),
        })
    }

    /// Set permit join duration
    pub async fn permit_join(&self, duration_secs: u8) -> Result<(), NetworkError> {
        self.transport
            .write_parameter(NetworkParameter::PermitJoin, &[duration_secs])
            .await?;
        Ok(())
    }

    /// Save devices to disk (spawns background task)
    fn save_devices(&self) {
        if let Some(path) = &self.data_path {
            let devices: Vec<ZigbeeDevice> =
                self.devices.iter().map(|r| r.value().clone()).collect();
            let path = path.clone();
            tokio::spawn(async move {
                if let Err(e) = persistence::save_devices(&path, &devices).await {
                    tracing::warn!("Failed to save devices: {}", e);
                }
            });
        }
    }

    /// Get all known devices
    pub fn get_devices(&self) -> Vec<ZigbeeDevice> {
        self.devices.iter().map(|r| r.value().clone()).collect()
    }

    /// Get a specific device by IEEE address
    pub fn get_device(&self, ieee: &[u8; 8]) -> Option<ZigbeeDevice> {
        self.devices.get(ieee).map(|r| r.value().clone())
    }

    /// Add or update a device
    pub fn upsert_device(&self, device: ZigbeeDevice) {
        let ieee = device.ieee_address;
        let is_new = !self.devices.contains_key(&ieee);

        self.devices.insert(ieee, device.clone());

        let event = if is_new {
            NetworkEvent::DeviceJoined(device)
        } else {
            NetworkEvent::DeviceUpdated { ieee_address: ieee }
        };

        let _ = self.event_tx.send(event);
        self.save_devices();
    }

    /// Remove a device
    pub fn remove_device(&self, ieee: &[u8; 8]) -> Option<ZigbeeDevice> {
        let removed = self.devices.remove(ieee).map(|(_, v)| v);
        if removed.is_some() {
            let _ = self.event_tx.send(NetworkEvent::DeviceLeft {
                ieee_address: *ieee,
            });
            self.save_devices();
        }
        removed
    }

    /// Send On/Off command to a device
    pub async fn send_on_off(
        &self,
        ieee: &[u8; 8],
        endpoint: u8,
        command: OnOffCommand,
    ) -> Result<(), NetworkError> {
        // Get the device to find its short address
        let device = self
            .devices
            .get(ieee)
            .ok_or_else(|| NetworkError::DeviceNotFound(format!("{ieee:02X?}")))?;

        let short_addr = device.nwk_address;
        let current_state = device.state_on;
        drop(device); // Release the lock

        // Build ZCL frame
        let zcl_frame = ZclFrame::on_off_command(1, command);
        let asdu = zcl_frame.serialize();

        // Build APS request
        let request = ApsDataRequest::new(1, short_addr, endpoint, clusters::ON_OFF, asdu);

        tracing::info!(
            "Sending {:?} command to device {:#06x}:{}",
            command,
            short_addr,
            endpoint
        );

        self.transport.send_aps_request(request).await?;

        // Determine new state and emit event
        let new_state = match command {
            OnOffCommand::On => Some(true),
            OnOffCommand::Off => Some(false),
            OnOffCommand::Toggle => current_state.map(|s| !s),
        };

        if let Some(state_on) = new_state {
            // Update device state
            if let Some(mut device) = self.devices.get_mut(ieee) {
                device.state_on = Some(state_on);
            }

            // Emit state change event
            let _ = self.event_tx.send(NetworkEvent::DeviceStateChanged {
                ieee_address: *ieee,
                endpoint,
                state_on,
            });

            // Persist
            self.save_devices();
        }

        Ok(())
    }

    /// Toggle a device
    pub async fn toggle_device(&self, ieee: &[u8; 8], endpoint: u8) -> Result<(), NetworkError> {
        self.send_on_off(ieee, endpoint, OnOffCommand::Toggle).await
    }

    /// Turn a device on
    pub async fn turn_on(&self, ieee: &[u8; 8], endpoint: u8) -> Result<(), NetworkError> {
        self.send_on_off(ieee, endpoint, OnOffCommand::On).await
    }

    /// Turn a device off
    pub async fn turn_off(&self, ieee: &[u8; 8], endpoint: u8) -> Result<(), NetworkError> {
        self.send_on_off(ieee, endpoint, OnOffCommand::Off).await
    }

    /// Request endpoint discovery for a device
    /// Sends Active Endpoints Request, response handled in event listener
    pub async fn discover_endpoints(&self, ieee: &[u8; 8]) -> Result<(), NetworkError> {
        let device = self
            .devices
            .get(ieee)
            .ok_or_else(|| NetworkError::DeviceNotFound(format!("{ieee:02X?}")))?;

        let short_addr = device.nwk_address;
        drop(device);

        tracing::info!(
            "Requesting active endpoints from device {:#06x}",
            short_addr
        );

        let request = ApsDataRequest::active_endpoints_request(1, short_addr, 1);
        self.transport.send_aps_request(request).await?;

        Ok(())
    }

    /// Request simple descriptor for a specific endpoint
    pub async fn discover_simple_descriptor(
        &self,
        ieee: &[u8; 8],
        endpoint: u8,
    ) -> Result<(), NetworkError> {
        let device = self
            .devices
            .get(ieee)
            .ok_or_else(|| NetworkError::DeviceNotFound(format!("{ieee:02X?}")))?;

        let short_addr = device.nwk_address;
        drop(device);

        tracing::info!(
            "Requesting simple descriptor for device {:#06x} endpoint {}",
            short_addr,
            endpoint
        );

        let request = ApsDataRequest::simple_descriptor_request(1, short_addr, endpoint, 1);
        self.transport.send_aps_request(request).await?;

        Ok(())
    }

    /// Update device metadata (friendly name and category)
    pub fn update_device_metadata(
        &self,
        ieee: &[u8; 8],
        friendly_name: Option<String>,
        category: Option<DeviceCategory>,
    ) -> Result<ZigbeeDevice, NetworkError> {
        let mut device = self
            .devices
            .get_mut(ieee)
            .ok_or_else(|| NetworkError::DeviceNotFound(format!("{ieee:02X?}")))?;

        if let Some(name) = friendly_name {
            device.friendly_name = if name.is_empty() { None } else { Some(name) };
        }
        if let Some(cat) = category {
            device.category = cat;
        }

        let updated_device = device.clone();
        drop(device);

        // Emit update event
        let _ = self.event_tx.send(NetworkEvent::DeviceUpdated {
            ieee_address: *ieee,
        });

        // Persist changes
        self.save_devices();

        Ok(updated_device)
    }
}
