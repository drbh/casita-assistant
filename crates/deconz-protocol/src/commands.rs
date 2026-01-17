//! deCONZ protocol command definitions

/// Command IDs for deCONZ serial protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommandId {
    /// APS data confirm (response to APS request)
    ApsDataConfirm = 0x04,
    /// Query device state
    DeviceState = 0x07,
    /// Change network state (connect/disconnect)
    ChangeNetworkState = 0x08,
    /// Read network parameter
    ReadParameter = 0x0A,
    /// Write network parameter
    WriteParameter = 0x0B,
    /// Query firmware version
    Version = 0x0D,
    /// Device state changed notification
    DeviceStateChanged = 0x0E,
    /// Send APS data request
    ApsDataRequest = 0x12,
    /// APS data indication (incoming data)
    ApsDataIndication = 0x17,
    /// Green Power data
    GreenPower = 0x19,
    /// MAC poll indication
    MacPoll = 0x1C,
    /// Neighbor table update
    NeighborUpdate = 0x1D,
    /// MAC beacon indication
    MacBeaconIndication = 0x1F,
}

impl CommandId {
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x04 => Some(CommandId::ApsDataConfirm),
            0x07 => Some(CommandId::DeviceState),
            0x08 => Some(CommandId::ChangeNetworkState),
            0x0A => Some(CommandId::ReadParameter),
            0x0B => Some(CommandId::WriteParameter),
            0x0D => Some(CommandId::Version),
            0x0E => Some(CommandId::DeviceStateChanged),
            0x12 => Some(CommandId::ApsDataRequest),
            0x17 => Some(CommandId::ApsDataIndication),
            0x19 => Some(CommandId::GreenPower),
            0x1C => Some(CommandId::MacPoll),
            0x1D => Some(CommandId::NeighborUpdate),
            0x1F => Some(CommandId::MacBeaconIndication),
            _ => None,
        }
    }
}

/// Network parameters that can be read/written
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NetworkParameter {
    /// MAC address (IEEE address, 8 bytes)
    MacAddress = 0x01,
    /// Network PAN ID (2 bytes)
    NwkPanId = 0x05,
    /// Network short address (2 bytes)
    NwkAddress = 0x07,
    /// Network extended PAN ID (8 bytes)
    NwkExtendedPanId = 0x08,
    /// Is this device the coordinator? (1 byte)
    ApsDesignedCoordinator = 0x09,
    /// Channel mask (4 bytes)
    ChannelMask = 0x0A,
    /// APS extended PAN ID (8 bytes)
    ApsExtendedPanId = 0x0B,
    /// Trust center address (8 bytes)
    TrustCenterAddress = 0x0E,
    /// Security mode (1 byte)
    SecurityMode = 0x10,
    /// Predefined network PAN ID (1 byte, bool)
    PredefinedNwkPanId = 0x15,
    /// Network key (16 bytes)
    NetworkKey = 0x18,
    /// Link key (16 bytes)
    LinkKey = 0x19,
    /// Current channel (1 byte)
    CurrentChannel = 0x1C,
    /// Permit join duration (1 byte)
    PermitJoin = 0x21,
    /// Protocol version (2 bytes)
    ProtocolVersion = 0x22,
    /// Network update ID (1 byte)
    NwkUpdateId = 0x24,
    /// Watchdog TTL (4 bytes)
    WatchdogTtl = 0x26,
}

impl NetworkParameter {
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(NetworkParameter::MacAddress),
            0x05 => Some(NetworkParameter::NwkPanId),
            0x07 => Some(NetworkParameter::NwkAddress),
            0x08 => Some(NetworkParameter::NwkExtendedPanId),
            0x09 => Some(NetworkParameter::ApsDesignedCoordinator),
            0x0A => Some(NetworkParameter::ChannelMask),
            0x0B => Some(NetworkParameter::ApsExtendedPanId),
            0x0E => Some(NetworkParameter::TrustCenterAddress),
            0x10 => Some(NetworkParameter::SecurityMode),
            0x15 => Some(NetworkParameter::PredefinedNwkPanId),
            0x18 => Some(NetworkParameter::NetworkKey),
            0x19 => Some(NetworkParameter::LinkKey),
            0x1C => Some(NetworkParameter::CurrentChannel),
            0x21 => Some(NetworkParameter::PermitJoin),
            0x22 => Some(NetworkParameter::ProtocolVersion),
            0x24 => Some(NetworkParameter::NwkUpdateId),
            0x26 => Some(NetworkParameter::WatchdogTtl),
            _ => None,
        }
    }

    /// Get the expected length of the parameter value
    #[must_use]
    pub fn value_length(&self) -> usize {
        match self {
            NetworkParameter::ApsDesignedCoordinator
            | NetworkParameter::SecurityMode
            | NetworkParameter::PredefinedNwkPanId
            | NetworkParameter::CurrentChannel
            | NetworkParameter::PermitJoin
            | NetworkParameter::NwkUpdateId => 1,
            NetworkParameter::NwkPanId
            | NetworkParameter::NwkAddress
            | NetworkParameter::ProtocolVersion => 2,
            NetworkParameter::ChannelMask | NetworkParameter::WatchdogTtl => 4,
            NetworkParameter::MacAddress
            | NetworkParameter::NwkExtendedPanId
            | NetworkParameter::ApsExtendedPanId
            | NetworkParameter::TrustCenterAddress => 8,
            NetworkParameter::NetworkKey | NetworkParameter::LinkKey => 16,
        }
    }
}

/// Network state change commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NetworkStateCommand {
    /// Bring network offline
    Offline = 0x00,
    /// Start network / connect
    Online = 0x02,
}
