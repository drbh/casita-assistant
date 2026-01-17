//! Common types used throughout the protocol

use thiserror::Error;

/// Protocol errors
#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Invalid frame: {0}")]
    InvalidFrame(String),

    #[error("CRC mismatch: expected {expected:04X}, got {actual:04X}")]
    CrcMismatch { expected: u16, actual: u16 },

    #[error("Frame too short: {0} bytes")]
    FrameTooShort(usize),

    #[error("Unknown command ID: {0:#04X}")]
    UnknownCommand(u8),

    #[error("Serial port error: {0}")]
    SerialError(#[from] std::io::Error),

    #[error("Request timeout")]
    Timeout,

    #[error("Transport not connected")]
    NotConnected,

    #[error("Device returned error status: {0:?}")]
    DeviceError(Status),
}

/// Device status codes from deCONZ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Status {
    Success = 0x00,
    Failure = 0x01,
    Busy = 0x02,
    Timeout = 0x03,
    Unsupported = 0x04,
    Error = 0x05,
    NoNetwork = 0x06,
    InvalidValue = 0x07,
}

impl TryFrom<u8> for Status {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, u8> {
        match value {
            0x00 => Ok(Status::Success),
            0x01 => Ok(Status::Failure),
            0x02 => Ok(Status::Busy),
            0x03 => Ok(Status::Timeout),
            0x04 => Ok(Status::Unsupported),
            0x05 => Ok(Status::Error),
            0x06 => Ok(Status::NoNetwork),
            0x07 => Ok(Status::InvalidValue),
            _ => Err(value),
        }
    }
}

/// Device state flags
#[derive(Debug, Clone, Copy)]
pub struct DeviceState {
    pub network_state: NetworkState,
    pub aps_data_confirm: bool,
    pub aps_data_indication: bool,
    pub configuration_changed: bool,
    pub aps_request_free_slots: bool,
}

impl DeviceState {
    #[must_use] pub fn from_byte(byte: u8) -> Self {
        Self {
            network_state: NetworkState::from_bits(byte & 0x03),
            aps_data_confirm: (byte & 0x04) != 0,
            aps_data_indication: (byte & 0x08) != 0,
            configuration_changed: (byte & 0x10) != 0,
            aps_request_free_slots: (byte & 0x20) != 0,
        }
    }
}

/// Network state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkState {
    Offline = 0,
    Joining = 1,
    Connected = 2,
    Leaving = 3,
}

impl NetworkState {
    #[must_use] pub fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            0 => NetworkState::Offline,
            1 => NetworkState::Joining,
            2 => NetworkState::Connected,
            3 => NetworkState::Leaving,
            _ => unreachable!(),
        }
    }
}

/// Platform identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    ConBee,
    ConBeeII,
    RaspBee,
    RaspBeeII,
    Unknown(u8),
}

impl From<u8> for Platform {
    fn from(value: u8) -> Self {
        match value {
            0x03 => Platform::RaspBee,
            0x05 => Platform::ConBee,
            0x06 => Platform::RaspBeeII,
            0x07 => Platform::ConBeeII,
            v => Platform::Unknown(v),
        }
    }
}

/// Firmware version information
#[derive(Debug, Clone)]
pub struct FirmwareVersion {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
    pub platform: Platform,
}

impl FirmwareVersion {
    #[must_use] pub fn from_u32(version: u32) -> Self {
        Self {
            major: ((version >> 24) & 0xFF) as u8,
            minor: ((version >> 16) & 0xFF) as u8,
            patch: ((version >> 8) & 0xFF) as u8,
            platform: Platform::from((version & 0xFF) as u8),
        }
    }
}

impl std::fmt::Display for FirmwareVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{}.{} ({:?})",
            self.major, self.minor, self.patch, self.platform
        )
    }
}

/// Address mode for APS frames
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AddressMode {
    Group = 0x01,
    Nwk = 0x02,
    Ieee = 0x03,
    NwkAndIeee = 0x04,
}

impl TryFrom<u8> for AddressMode {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, u8> {
        match value {
            0x01 => Ok(AddressMode::Group),
            0x02 => Ok(AddressMode::Nwk),
            0x03 => Ok(AddressMode::Ieee),
            0x04 => Ok(AddressMode::NwkAndIeee),
            _ => Err(value),
        }
    }
}

/// ZDO cluster IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ZdoCluster {
    DeviceAnnce = 0x0013,
    NodeDescReq = 0x0002,
    NodeDescRsp = 0x8002,
    SimpleDescReq = 0x0004,
    SimpleDescRsp = 0x8004,
    ActiveEpReq = 0x0005,
    ActiveEpRsp = 0x8005,
}

/// APS Data Indication - parsed incoming `ZigBee` message
#[derive(Debug, Clone)]
pub struct ApsDataIndication {
    pub device_state: DeviceState,
    pub dest_addr_mode: AddressMode,
    pub dest_addr: u16,
    pub dest_endpoint: u8,
    pub src_addr_mode: AddressMode,
    pub src_short_addr: u16,
    pub src_ieee_addr: Option<[u8; 8]>,
    pub src_endpoint: u8,
    pub profile_id: u16,
    pub cluster_id: u16,
    pub asdu: Vec<u8>,
    pub lqi: u8,
    pub rssi: i8,
}

impl ApsDataIndication {
    /// Parse APS Data Indication from raw payload
    pub fn parse(data: &[u8]) -> Result<Self, ProtocolError> {
        if data.len() < 15 {
            return Err(ProtocolError::FrameTooShort(data.len()));
        }

        let mut idx = 0;

        // Skip payload_len (2 bytes) - we already have the data
        let _payload_len = u16::from_le_bytes([data[idx], data[idx + 1]]);
        idx += 2;

        // Device state
        let device_state = DeviceState::from_byte(data[idx]);
        idx += 1;

        // Destination address
        let dest_addr_mode = AddressMode::try_from(data[idx])
            .map_err(|v| ProtocolError::InvalidFrame(format!("Unknown dest addr mode: {v}")))?;
        idx += 1;

        let dest_addr = match dest_addr_mode {
            AddressMode::Nwk | AddressMode::Group => {
                let addr = u16::from_le_bytes([data[idx], data[idx + 1]]);
                idx += 2;
                addr
            }
            AddressMode::Ieee => {
                idx += 8; // Skip 8-byte IEEE
                0
            }
            AddressMode::NwkAndIeee => {
                let addr = u16::from_le_bytes([data[idx], data[idx + 1]]);
                idx += 10; // 2 short + 8 IEEE
                addr
            }
        };

        let dest_endpoint = data[idx];
        idx += 1;

        // Source address
        let src_addr_mode = AddressMode::try_from(data[idx])
            .map_err(|v| ProtocolError::InvalidFrame(format!("Unknown src addr mode: {v}")))?;
        idx += 1;

        let (src_short_addr, src_ieee_addr) = match src_addr_mode {
            AddressMode::Nwk | AddressMode::Group => {
                let addr = u16::from_le_bytes([data[idx], data[idx + 1]]);
                idx += 2;
                (addr, None)
            }
            AddressMode::Ieee => {
                let mut ieee = [0u8; 8];
                ieee.copy_from_slice(&data[idx..idx + 8]);
                idx += 8;
                (0, Some(ieee))
            }
            AddressMode::NwkAndIeee => {
                let short = u16::from_le_bytes([data[idx], data[idx + 1]]);
                idx += 2;
                let mut ieee = [0u8; 8];
                ieee.copy_from_slice(&data[idx..idx + 8]);
                idx += 8;
                (short, Some(ieee))
            }
        };

        let src_endpoint = data[idx];
        idx += 1;

        // Profile and cluster
        let profile_id = u16::from_le_bytes([data[idx], data[idx + 1]]);
        idx += 2;
        let cluster_id = u16::from_le_bytes([data[idx], data[idx + 1]]);
        idx += 2;

        // ASDU
        let asdu_len = u16::from_le_bytes([data[idx], data[idx + 1]]) as usize;
        idx += 2;

        if idx + asdu_len > data.len() {
            return Err(ProtocolError::FrameTooShort(data.len()));
        }

        let asdu = data[idx..idx + asdu_len].to_vec();
        idx += asdu_len;

        // LQI and RSSI (may not be present in all firmware versions)
        let lqi = if idx < data.len() { data[idx] } else { 0 };
        let rssi = if idx + 1 < data.len() {
            data[idx + 1] as i8
        } else {
            0
        };

        Ok(Self {
            device_state,
            dest_addr_mode,
            dest_addr,
            dest_endpoint,
            src_addr_mode,
            src_short_addr,
            src_ieee_addr,
            src_endpoint,
            profile_id,
            cluster_id,
            asdu,
            lqi,
            rssi,
        })
    }

    /// Format IEEE address as string (colon-separated hex)
    #[must_use] pub fn format_ieee(ieee: &[u8; 8]) -> String {
        // IEEE is stored little-endian, display big-endian
        format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            ieee[7], ieee[6], ieee[5], ieee[4], ieee[3], ieee[2], ieee[1], ieee[0]
        )
    }
}

/// Device Announcement from ZDO cluster 0x0013
#[derive(Debug, Clone)]
pub struct DeviceAnnouncement {
    pub tsn: u8,
    pub short_addr: u16,
    pub ieee_addr: [u8; 8],
    pub capability: u8,
}

impl DeviceAnnouncement {
    /// Parse device announcement from ASDU
    pub fn parse(asdu: &[u8]) -> Result<Self, ProtocolError> {
        if asdu.len() < 12 {
            return Err(ProtocolError::FrameTooShort(asdu.len()));
        }

        let tsn = asdu[0];
        let short_addr = u16::from_le_bytes([asdu[1], asdu[2]]);
        let mut ieee_addr = [0u8; 8];
        ieee_addr.copy_from_slice(&asdu[3..11]);
        let capability = asdu[11];

        Ok(Self {
            tsn,
            short_addr,
            ieee_addr,
            capability,
        })
    }

    /// Check if device is a router (FFD)
    #[must_use] pub fn is_router(&self) -> bool {
        (self.capability & 0x02) != 0
    }

    /// Check if device is mains powered
    #[must_use] pub fn is_mains_powered(&self) -> bool {
        (self.capability & 0x04) != 0
    }

    /// Check if receiver is on when idle
    #[must_use] pub fn rx_on_when_idle(&self) -> bool {
        (self.capability & 0x08) != 0
    }
}

/// Active Endpoints Response from ZDO cluster 0x8005
#[derive(Debug, Clone)]
pub struct ActiveEndpointsResponse {
    pub tsn: u8,
    pub status: u8,
    pub nwk_addr: u16,
    pub endpoints: Vec<u8>,
}

impl ActiveEndpointsResponse {
    /// Parse from ASDU
    pub fn parse(asdu: &[u8]) -> Result<Self, ProtocolError> {
        if asdu.len() < 4 {
            return Err(ProtocolError::FrameTooShort(asdu.len()));
        }

        let tsn = asdu[0];
        let status = asdu[1];
        let nwk_addr = u16::from_le_bytes([asdu[2], asdu[3]]);

        let endpoints = if status == 0 && asdu.len() > 4 {
            let ep_count = asdu[4] as usize;
            if asdu.len() >= 5 + ep_count {
                asdu[5..5 + ep_count].to_vec()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        Ok(Self {
            tsn,
            status,
            nwk_addr,
            endpoints,
        })
    }
}

/// Simple Descriptor Response from ZDO cluster 0x8004
#[derive(Debug, Clone)]
pub struct SimpleDescriptorResponse {
    pub tsn: u8,
    pub status: u8,
    pub nwk_addr: u16,
    pub endpoint: u8,
    pub profile_id: u16,
    pub device_id: u16,
    pub device_version: u8,
    pub in_clusters: Vec<u16>,
    pub out_clusters: Vec<u16>,
}

impl SimpleDescriptorResponse {
    /// Parse from ASDU
    pub fn parse(asdu: &[u8]) -> Result<Self, ProtocolError> {
        if asdu.len() < 5 {
            return Err(ProtocolError::FrameTooShort(asdu.len()));
        }

        let tsn = asdu[0];
        let status = asdu[1];
        let nwk_addr = u16::from_le_bytes([asdu[2], asdu[3]]);

        if status != 0 || asdu.len() < 6 {
            return Ok(Self {
                tsn,
                status,
                nwk_addr,
                endpoint: 0,
                profile_id: 0,
                device_id: 0,
                device_version: 0,
                in_clusters: Vec::new(),
                out_clusters: Vec::new(),
            });
        }

        let _desc_len = asdu[4];
        let mut idx = 5;

        if asdu.len() < idx + 6 {
            return Err(ProtocolError::FrameTooShort(asdu.len()));
        }

        let endpoint = asdu[idx];
        idx += 1;

        let profile_id = u16::from_le_bytes([asdu[idx], asdu[idx + 1]]);
        idx += 2;

        let device_id = u16::from_le_bytes([asdu[idx], asdu[idx + 1]]);
        idx += 2;

        let device_version = asdu[idx] & 0x0F;
        idx += 1;

        // Input clusters
        let in_cluster_count = if idx < asdu.len() {
            asdu[idx] as usize
        } else {
            0
        };
        idx += 1;

        let mut in_clusters = Vec::with_capacity(in_cluster_count);
        for _ in 0..in_cluster_count {
            if idx + 2 <= asdu.len() {
                in_clusters.push(u16::from_le_bytes([asdu[idx], asdu[idx + 1]]));
                idx += 2;
            }
        }

        // Output clusters
        let out_cluster_count = if idx < asdu.len() {
            asdu[idx] as usize
        } else {
            0
        };
        idx += 1;

        let mut out_clusters = Vec::with_capacity(out_cluster_count);
        for _ in 0..out_cluster_count {
            if idx + 2 <= asdu.len() {
                out_clusters.push(u16::from_le_bytes([asdu[idx], asdu[idx + 1]]));
                idx += 2;
            }
        }

        Ok(Self {
            tsn,
            status,
            nwk_addr,
            endpoint,
            profile_id,
            device_id,
            device_version,
            in_clusters,
            out_clusters,
        })
    }
}

/// ZCL On/Off cluster commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OnOffCommand {
    Off = 0x00,
    On = 0x01,
    Toggle = 0x02,
}

/// ZCL cluster IDs
pub mod clusters {
    pub const ON_OFF: u16 = 0x0006;
    pub const LEVEL_CONTROL: u16 = 0x0008;
    pub const COLOR_CONTROL: u16 = 0x0300;
}

/// ZCL profile IDs
pub mod profiles {
    pub const ZDO: u16 = 0x0000;
    pub const HOME_AUTOMATION: u16 = 0x0104;
}

/// APS Data Request for sending commands to devices
#[derive(Debug, Clone)]
pub struct ApsDataRequest {
    pub request_id: u8,
    pub dest_addr_mode: AddressMode,
    pub dest_short_addr: u16,
    pub dest_endpoint: u8,
    pub profile_id: u16,
    pub cluster_id: u16,
    pub src_endpoint: u8,
    pub asdu: Vec<u8>,
    pub tx_options: u8,
    pub radius: u8,
}

impl ApsDataRequest {
    /// Create a new APS data request
    #[must_use] pub fn new(
        request_id: u8,
        dest_short_addr: u16,
        dest_endpoint: u8,
        cluster_id: u16,
        asdu: Vec<u8>,
    ) -> Self {
        Self {
            request_id,
            dest_addr_mode: AddressMode::Nwk,
            dest_short_addr,
            dest_endpoint,
            profile_id: profiles::HOME_AUTOMATION,
            cluster_id,
            src_endpoint: 0x01, // Default source endpoint
            asdu,
            tx_options: 0x04, // APS ACK requested
            radius: 0x00,     // Use network default
        }
    }

    /// Create a ZDO Active Endpoints Request
    #[must_use] pub fn active_endpoints_request(request_id: u8, dest_short_addr: u16, tsn: u8) -> Self {
        // ASDU: TSN (1 byte) + NWK address of interest (2 bytes LE)
        let mut asdu = vec![tsn];
        asdu.extend_from_slice(&dest_short_addr.to_le_bytes());

        Self {
            request_id,
            dest_addr_mode: AddressMode::Nwk,
            dest_short_addr,
            dest_endpoint: 0x00, // ZDO endpoint
            profile_id: profiles::ZDO,
            cluster_id: ZdoCluster::ActiveEpReq as u16,
            src_endpoint: 0x00, // ZDO endpoint
            asdu,
            tx_options: 0x00, // No ACK for ZDO
            radius: 0x00,
        }
    }

    /// Create a ZDO Simple Descriptor Request
    #[must_use] pub fn simple_descriptor_request(
        request_id: u8,
        dest_short_addr: u16,
        endpoint: u8,
        tsn: u8,
    ) -> Self {
        // ASDU: TSN (1 byte) + NWK address (2 bytes LE) + endpoint (1 byte)
        let mut asdu = vec![tsn];
        asdu.extend_from_slice(&dest_short_addr.to_le_bytes());
        asdu.push(endpoint);

        Self {
            request_id,
            dest_addr_mode: AddressMode::Nwk,
            dest_short_addr,
            dest_endpoint: 0x00, // ZDO endpoint
            profile_id: profiles::ZDO,
            cluster_id: ZdoCluster::SimpleDescReq as u16,
            src_endpoint: 0x00, // ZDO endpoint
            asdu,
            tx_options: 0x00, // No ACK for ZDO
            radius: 0x00,
        }
    }

    /// Serialize to bytes for sending
    #[must_use] pub fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::new();

        // Payload length (will be filled at the end)
        let payload_start = data.len();
        data.extend_from_slice(&0u16.to_le_bytes()); // placeholder

        // Request ID
        data.push(self.request_id);

        // Flags (0x00 = normal request)
        data.push(0x00);

        // Destination address mode
        data.push(self.dest_addr_mode as u8);

        // Destination address (short address for NWK mode)
        data.extend_from_slice(&self.dest_short_addr.to_le_bytes());

        // Destination endpoint
        data.push(self.dest_endpoint);

        // Profile ID
        data.extend_from_slice(&self.profile_id.to_le_bytes());

        // Cluster ID
        data.extend_from_slice(&self.cluster_id.to_le_bytes());

        // Source endpoint
        data.push(self.src_endpoint);

        // ASDU length
        data.extend_from_slice(&(self.asdu.len() as u16).to_le_bytes());

        // ASDU (ZCL frame)
        data.extend_from_slice(&self.asdu);

        // TX options
        data.push(self.tx_options);

        // Radius
        data.push(self.radius);

        // Fill in payload length (everything after the 2-byte length field)
        let payload_len = (data.len() - 2) as u16;
        data[payload_start..payload_start + 2].copy_from_slice(&payload_len.to_le_bytes());

        data
    }
}

/// ZCL frame (Zigbee Cluster Library)
#[derive(Debug, Clone)]
pub struct ZclFrame {
    frame_control: u8,
    manufacturer_code: Option<u16>,
    transaction_seq: u8,
    command_id: u8,
    payload: Vec<u8>,
}

impl ZclFrame {
    /// Parse a ZCL frame from raw ASDU bytes
    pub fn parse(data: &[u8]) -> Result<Self, ProtocolError> {
        if data.len() < 3 {
            return Err(ProtocolError::FrameTooShort(data.len()));
        }

        let frame_control = data[0];
        let mut idx = 1;

        // Check for manufacturer-specific (bit 2)
        let manufacturer_code = if (frame_control & 0x04) != 0 {
            if data.len() < idx + 2 {
                return Err(ProtocolError::FrameTooShort(data.len()));
            }
            let code = u16::from_le_bytes([data[idx], data[idx + 1]]);
            idx += 2;
            Some(code)
        } else {
            None
        };

        if data.len() < idx + 2 {
            return Err(ProtocolError::FrameTooShort(data.len()));
        }

        let transaction_seq = data[idx];
        idx += 1;
        let command_id = data[idx];
        idx += 1;

        let payload = data[idx..].to_vec();

        Ok(Self {
            frame_control,
            manufacturer_code,
            transaction_seq,
            command_id,
            payload,
        })
    }

    /// Get frame control byte
    #[must_use] pub fn frame_control(&self) -> u8 {
        self.frame_control
    }

    /// Check if this is a cluster-specific command (vs global)
    #[must_use] pub fn is_cluster_specific(&self) -> bool {
        (self.frame_control & 0x03) == 0x01
    }

    /// Check if this is from server to client (vs client to server)
    #[must_use] pub fn is_from_server(&self) -> bool {
        (self.frame_control & 0x08) != 0
    }

    /// Get the command ID
    #[must_use] pub fn command_id(&self) -> u8 {
        self.command_id
    }

    /// Get the payload
    #[must_use] pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Create a cluster-specific command frame (client to server)
    #[must_use] pub fn cluster_command(transaction_seq: u8, command_id: u8) -> Self {
        Self {
            frame_control: 0x01, // Cluster-specific, client-to-server, disable default response
            manufacturer_code: None,
            transaction_seq,
            command_id,
            payload: Vec::new(),
        }
    }

    /// Create an On/Off cluster command
    #[must_use] pub fn on_off_command(transaction_seq: u8, cmd: OnOffCommand) -> Self {
        Self::cluster_command(transaction_seq, cmd as u8)
    }

    /// Serialize to bytes
    #[must_use] pub fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(self.frame_control);
        if let Some(mfr) = self.manufacturer_code {
            data.extend_from_slice(&mfr.to_le_bytes());
        }
        data.push(self.transaction_seq);
        data.push(self.command_id);
        data.extend_from_slice(&self.payload);
        data
    }
}
