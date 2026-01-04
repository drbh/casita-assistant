//! ZCL (Zigbee Cluster Library) definitions

/// Common ZCL cluster IDs
pub mod id {
    // General Clusters
    pub const BASIC: u16 = 0x0000;
    pub const POWER_CONFIG: u16 = 0x0001;
    pub const DEVICE_TEMP: u16 = 0x0002;
    pub const IDENTIFY: u16 = 0x0003;
    pub const GROUPS: u16 = 0x0004;
    pub const SCENES: u16 = 0x0005;
    pub const ON_OFF: u16 = 0x0006;
    pub const ON_OFF_SWITCH_CONFIG: u16 = 0x0007;
    pub const LEVEL_CONTROL: u16 = 0x0008;
    pub const ALARMS: u16 = 0x0009;
    pub const TIME: u16 = 0x000A;

    // Lighting Clusters
    pub const COLOR_CONTROL: u16 = 0x0300;
    pub const BALLAST_CONFIG: u16 = 0x0301;

    // Measurement Clusters
    pub const ILLUMINANCE_MEASUREMENT: u16 = 0x0400;
    pub const ILLUMINANCE_LEVEL_SENSING: u16 = 0x0401;
    pub const TEMPERATURE_MEASUREMENT: u16 = 0x0402;
    pub const PRESSURE_MEASUREMENT: u16 = 0x0403;
    pub const FLOW_MEASUREMENT: u16 = 0x0404;
    pub const HUMIDITY_MEASUREMENT: u16 = 0x0405;
    pub const OCCUPANCY_SENSING: u16 = 0x0406;

    // Security Clusters
    pub const IAS_ZONE: u16 = 0x0500;
    pub const IAS_ACE: u16 = 0x0501;
    pub const IAS_WD: u16 = 0x0502;

    // HVAC Clusters
    pub const THERMOSTAT: u16 = 0x0201;
    pub const FAN_CONTROL: u16 = 0x0202;

    // Closures Clusters
    pub const DOOR_LOCK: u16 = 0x0101;
    pub const WINDOW_COVERING: u16 = 0x0102;

    // Smart Energy
    pub const METERING: u16 = 0x0702;
    pub const ELECTRICAL_MEASUREMENT: u16 = 0x0B04;
}

/// Basic cluster attributes
pub mod basic_attrs {
    pub const ZCL_VERSION: u16 = 0x0000;
    pub const APPLICATION_VERSION: u16 = 0x0001;
    pub const STACK_VERSION: u16 = 0x0002;
    pub const HW_VERSION: u16 = 0x0003;
    pub const MANUFACTURER_NAME: u16 = 0x0004;
    pub const MODEL_IDENTIFIER: u16 = 0x0005;
    pub const DATE_CODE: u16 = 0x0006;
    pub const POWER_SOURCE: u16 = 0x0007;
    pub const SW_BUILD_ID: u16 = 0x4000;
}

/// On/Off cluster commands
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum OnOffCommand {
    Off = 0x00,
    On = 0x01,
    Toggle = 0x02,
}

/// Level Control cluster commands
#[derive(Debug, Clone)]
pub enum LevelCommand {
    MoveToLevel {
        level: u8,
        transition_time: u16,
    },
    Move {
        mode: u8,
        rate: u8,
    },
    Step {
        mode: u8,
        step_size: u8,
        transition_time: u16,
    },
    Stop,
    MoveToLevelWithOnOff {
        level: u8,
        transition_time: u16,
    },
}

/// Color Control cluster commands
#[derive(Debug, Clone)]
pub enum ColorCommand {
    MoveToHue {
        hue: u8,
        direction: u8,
        transition_time: u16,
    },
    MoveToSaturation {
        saturation: u8,
        transition_time: u16,
    },
    MoveToHueAndSaturation {
        hue: u8,
        saturation: u8,
        transition_time: u16,
    },
    MoveToColor {
        x: u16,
        y: u16,
        transition_time: u16,
    },
    MoveToColorTemperature {
        color_temp_mireds: u16,
        transition_time: u16,
    },
}

/// ZCL Frame types
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum FrameType {
    Global = 0x00,
    ClusterSpecific = 0x01,
}

/// ZCL Direction
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Direction {
    ClientToServer = 0x00,
    ServerToClient = 0x01,
}

/// ZCL Global commands
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum GlobalCommand {
    ReadAttributes = 0x00,
    ReadAttributesResponse = 0x01,
    WriteAttributes = 0x02,
    WriteAttributesUndivided = 0x03,
    WriteAttributesResponse = 0x04,
    WriteAttributesNoResponse = 0x05,
    ConfigureReporting = 0x06,
    ConfigureReportingResponse = 0x07,
    ReadReportingConfig = 0x08,
    ReadReportingConfigResponse = 0x09,
    ReportAttributes = 0x0A,
    DefaultResponse = 0x0B,
    DiscoverAttributes = 0x0C,
    DiscoverAttributesResponse = 0x0D,
}

/// ZCL data types
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DataType {
    NoData = 0x00,
    Data8 = 0x08,
    Data16 = 0x09,
    Data24 = 0x0A,
    Data32 = 0x0B,
    Boolean = 0x10,
    Bitmap8 = 0x18,
    Bitmap16 = 0x19,
    Bitmap24 = 0x1A,
    Bitmap32 = 0x1B,
    Uint8 = 0x20,
    Uint16 = 0x21,
    Uint24 = 0x22,
    Uint32 = 0x23,
    Int8 = 0x28,
    Int16 = 0x29,
    Int24 = 0x2A,
    Int32 = 0x2B,
    Enum8 = 0x30,
    Enum16 = 0x31,
    Float16 = 0x38,
    Float32 = 0x39,
    Float64 = 0x3A,
    String = 0x42,
    Array = 0x48,
    Struct = 0x4C,
    Ieee = 0xF0,
}
