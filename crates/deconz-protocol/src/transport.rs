//! Async serial transport for deCONZ protocol

use crate::commands::{CommandId, NetworkParameter};
use crate::frame::Frame;
use crate::slip::{SlipDecoder, SlipEncoder};
use crate::types::{
    ApsDataIndication, ApsDataRequest, DeviceAnnouncement, DeviceState, FirmwareVersion,
    ProtocolError, Status,
};

use serial2::SerialPort;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};

/// Default baud rate for ConBee II
pub const BAUD_RATE: u32 = 115200;

/// Default request timeout
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Events from the deCONZ device
#[derive(Debug, Clone)]
pub enum DeconzEvent {
    /// Device state changed
    DeviceStateChanged(DeviceState),
    /// APS data indication available
    ApsDataAvailable,
    /// APS data received (raw)
    ApsDataReceived { data: Vec<u8> },
    /// Parsed APS data indication
    ApsIndication(ApsDataIndication),
    /// Device announced on the network
    DeviceAnnounced {
        ieee_addr: [u8; 8],
        short_addr: u16,
        capability: u8,
    },
    /// MAC poll from a device
    MacPoll { short_addr: u16 },
}

/// Pending request waiting for response
struct PendingRequest {
    response_tx: oneshot::Sender<Result<Frame, ProtocolError>>,
}

/// Command to send to the writer task
enum WriteCommand {
    Send(Vec<u8>),
    Shutdown,
}

/// Received frame from reader thread
struct ReceivedFrame {
    data: Vec<u8>,
}

/// Async transport for communicating with deCONZ devices
pub struct DeconzTransport {
    /// Channel to send data to the writer task
    write_tx: mpsc::Sender<WriteCommand>,
    /// Sequence counter
    sequence: AtomicU8,
    /// Pending requests awaiting responses
    pending: Arc<Mutex<HashMap<u8, PendingRequest>>>,
    /// Event sender for unsolicited messages
    event_tx: broadcast::Sender<DeconzEvent>,
}

impl DeconzTransport {
    /// Connect to a deCONZ device at the given serial port path
    pub async fn connect(path: &str) -> Result<Self, ProtocolError> {
        tracing::info!("Connecting to deCONZ device at {}", path);

        // Open serial port
        let mut port = SerialPort::open(path, BAUD_RATE).map_err(ProtocolError::SerialError)?;

        // Set read timeout to make reads non-blocking (short timeout)
        port.set_read_timeout(Duration::from_millis(100))
            .map_err(ProtocolError::SerialError)?;

        // Clone port for reader (serial2 supports clone)
        let reader_port = port.try_clone().map_err(ProtocolError::SerialError)?;

        let pending: Arc<Mutex<HashMap<u8, PendingRequest>>> = Arc::new(Mutex::new(HashMap::new()));
        let (event_tx, _) = broadcast::channel(64);
        let (write_tx, write_rx) = mpsc::channel(32);
        let (frame_tx, frame_rx) = mpsc::channel::<ReceivedFrame>(64);

        // Spawn writer task
        let writer_port = port;
        tokio::spawn(Self::writer_task(writer_port, write_rx));

        // Spawn reader thread (sends frames via channel)
        std::thread::spawn(move || {
            Self::reader_thread(reader_port, frame_tx);
        });

        // Spawn frame handler task (processes frames from reader thread)
        let pending_clone = pending.clone();
        let event_tx_clone = event_tx.clone();
        tokio::spawn(Self::frame_handler_task(
            frame_rx,
            pending_clone,
            event_tx_clone,
        ));

        tracing::info!("Connected to deCONZ device");

        Ok(Self {
            write_tx,
            sequence: AtomicU8::new(1),
            pending,
            event_tx,
        })
    }

    /// Writer task - runs in tokio runtime
    async fn writer_task(port: SerialPort, mut rx: mpsc::Receiver<WriteCommand>) {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                WriteCommand::Send(data) => {
                    tracing::debug!("Writing {} bytes to serial port", data.len());
                    match port.write_all(&data) {
                        Ok(_) => tracing::debug!("Write successful"),
                        Err(e) => tracing::error!("Write error: {}", e),
                    }
                    if let Err(e) = port.flush() {
                        tracing::error!("Flush error: {}", e);
                    }
                }
                WriteCommand::Shutdown => break,
            }
        }
        tracing::debug!("Writer task shutting down");
    }

    /// Reader thread - runs in a standard thread with blocking I/O
    fn reader_thread(port: SerialPort, frame_tx: mpsc::Sender<ReceivedFrame>) {
        tracing::debug!("Reader thread started");
        let mut buffer = [0u8; 1024];
        let mut decoder = SlipDecoder::new();

        loop {
            match port.read(&mut buffer) {
                Ok(0) => {
                    tracing::warn!("Serial port closed");
                    break;
                }
                Ok(n) => {
                    tracing::debug!("Read {} bytes: {:02X?}", n, &buffer[..n]);
                    let frames = decoder.feed(&buffer[..n]);
                    for frame_data in frames {
                        tracing::debug!("Decoded frame: {:02X?}", &frame_data);
                        // Send frame to async handler via channel
                        if frame_tx
                            .blocking_send(ReceivedFrame { data: frame_data })
                            .is_err()
                        {
                            tracing::warn!("Frame channel closed");
                            return;
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    continue;
                }
                Err(ref e) if e.raw_os_error() == Some(libc::EAGAIN) => {
                    continue;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(e) => {
                    tracing::error!("Serial read error: {}", e);
                    break;
                }
            }
        }
        tracing::debug!("Reader thread shutting down");
    }

    /// Frame handler task - processes frames from reader thread
    async fn frame_handler_task(
        mut frame_rx: mpsc::Receiver<ReceivedFrame>,
        pending: Arc<Mutex<HashMap<u8, PendingRequest>>>,
        event_tx: broadcast::Sender<DeconzEvent>,
    ) {
        while let Some(received) = frame_rx.recv().await {
            if let Err(e) = Self::handle_frame(&received.data, &pending, &event_tx).await {
                tracing::warn!("Error handling frame: {}", e);
            }
        }
        tracing::debug!("Frame handler task shutting down");
    }

    /// Handle a received frame
    async fn handle_frame(
        data: &[u8],
        pending: &Arc<Mutex<HashMap<u8, PendingRequest>>>,
        event_tx: &broadcast::Sender<DeconzEvent>,
    ) -> Result<(), ProtocolError> {
        let frame = Frame::deserialize(data)?;
        tracing::debug!(
            "Received frame: cmd={:?} seq={} payload_len={}",
            frame.command_id,
            frame.sequence,
            frame.payload.len()
        );

        // Check if this is a response to a pending request
        let mut pending_guard = pending.lock().await;
        if let Some(req) = pending_guard.remove(&frame.sequence) {
            drop(pending_guard);
            let _ = req.response_tx.send(Ok(frame));
            return Ok(());
        }
        drop(pending_guard);

        // Handle unsolicited messages
        match frame.command_id {
            CommandId::DeviceStateChanged => {
                if !frame.payload.is_empty() {
                    let state = DeviceState::from_byte(frame.payload[0]);
                    let _ = event_tx.send(DeconzEvent::DeviceStateChanged(state));

                    if state.aps_data_indication {
                        let _ = event_tx.send(DeconzEvent::ApsDataAvailable);
                    }
                }
            }
            CommandId::ApsDataIndication => {
                tracing::debug!("APS Data Indication received: {:02X?}", frame.payload);
                let _ = event_tx.send(DeconzEvent::ApsDataReceived {
                    data: frame.payload.clone(),
                });

                // Try to parse the indication
                if let Ok(indication) = ApsDataIndication::parse(&frame.payload) {
                    tracing::info!(
                        "APS Indication: cluster={:#06x} profile={:#06x} src={:#06x}",
                        indication.cluster_id,
                        indication.profile_id,
                        indication.src_short_addr
                    );

                    // Check for device announcement (ZDO cluster 0x0013)
                    if indication.profile_id == 0x0000 && indication.cluster_id == 0x0013 {
                        if let Ok(announce) = DeviceAnnouncement::parse(&indication.asdu) {
                            let ieee_str = ApsDataIndication::format_ieee(&announce.ieee_addr);
                            tracing::info!(
                                "Device Announced: IEEE={} short={:#06x} cap={:#04x} router={} mains={}",
                                ieee_str,
                                announce.short_addr,
                                announce.capability,
                                announce.is_router(),
                                announce.is_mains_powered()
                            );
                            let _ = event_tx.send(DeconzEvent::DeviceAnnounced {
                                ieee_addr: announce.ieee_addr,
                                short_addr: announce.short_addr,
                                capability: announce.capability,
                            });
                        }
                    }

                    let _ = event_tx.send(DeconzEvent::ApsIndication(indication));
                }
            }
            CommandId::MacPoll => {
                // Parse MAC poll - contains source address info
                if frame.payload.len() >= 3 {
                    let short_addr = u16::from_le_bytes([frame.payload[1], frame.payload[2]]);
                    tracing::debug!("MacPoll from device: {:#06x}", short_addr);
                    let _ = event_tx.send(DeconzEvent::MacPoll { short_addr });
                }
            }
            _ => {
                tracing::debug!("Unhandled unsolicited frame: {:?}", frame.command_id);
            }
        }

        Ok(())
    }

    /// Send a request and wait for response
    pub async fn request(
        &self,
        command_id: CommandId,
        payload: Vec<u8>,
    ) -> Result<Frame, ProtocolError> {
        self.request_timeout(command_id, payload, DEFAULT_TIMEOUT)
            .await
    }

    /// Send a request with custom timeout
    pub async fn request_timeout(
        &self,
        command_id: CommandId,
        payload: Vec<u8>,
        timeout: Duration,
    ) -> Result<Frame, ProtocolError> {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);
        let frame = Frame::new(command_id, sequence, payload);
        let data = SlipEncoder::encode(&frame.serialize());

        // Set up response channel
        let (response_tx, response_rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(sequence, PendingRequest { response_tx });
        }

        // Send the frame
        tracing::debug!("Sending raw data: {:02X?}", &data);

        self.write_tx
            .send(WriteCommand::Send(data))
            .await
            .map_err(|_| ProtocolError::NotConnected)?;

        tracing::debug!(
            "Sent frame: cmd={:?} seq={} payload_len={}",
            command_id,
            sequence,
            frame.payload.len()
        );

        // Wait for response with timeout
        match tokio::time::timeout(timeout, response_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(ProtocolError::Timeout),
            Err(_) => {
                // Remove pending request on timeout
                let mut pending = self.pending.lock().await;
                pending.remove(&sequence);
                Err(ProtocolError::Timeout)
            }
        }
    }

    /// Subscribe to device events
    pub fn subscribe(&self) -> broadcast::Receiver<DeconzEvent> {
        self.event_tx.subscribe()
    }

    /// Query firmware version
    pub async fn get_version(&self) -> Result<FirmwareVersion, ProtocolError> {
        // Try to get version via ReadParameter(ProtocolVersion) as fallback
        // since the Version command may not work on all firmware versions
        let version_data = self
            .read_parameter(NetworkParameter::ProtocolVersion)
            .await?;

        tracing::debug!("ProtocolVersion data: {:02X?}", version_data);

        // ProtocolVersion is 2 bytes, but we need to construct a FirmwareVersion
        // For now, use the protocol version as a simplified version
        if version_data.len() >= 2 {
            let protocol_version = u16::from_le_bytes([version_data[0], version_data[1]]);
            // Create a pseudo firmware version from protocol version
            Ok(FirmwareVersion::from_u32(protocol_version as u32))
        } else {
            Err(ProtocolError::InvalidFrame(
                "Protocol version response too short".to_string(),
            ))
        }
    }

    /// Query device state
    pub async fn get_device_state(&self) -> Result<DeviceState, ProtocolError> {
        // DeviceState request with reserved byte (0x00) as per protocol spec
        let response = self.request(CommandId::DeviceState, vec![0x00]).await?;

        // Response payload format: device_state(1) + optional padding
        // The device_state byte is the first byte
        if response.payload.is_empty() {
            return Err(ProtocolError::InvalidFrame(
                "Device state response empty".to_string(),
            ));
        }

        tracing::debug!("DeviceState payload: {:02X?}", response.payload);
        Ok(DeviceState::from_byte(response.payload[0]))
    }

    /// Read a network parameter
    pub async fn read_parameter(&self, param: NetworkParameter) -> Result<Vec<u8>, ProtocolError> {
        // Request format: payload_len(2 LE) + param_id(1)
        // payload_len = 1 (just the param_id byte)
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u16.to_le_bytes()); // payload_len = 1
        payload.push(param as u8);

        let response = self.request(CommandId::ReadParameter, payload).await?;

        tracing::debug!(
            "ReadParameter({:?}) response: status={}, payload={:02X?}",
            param,
            response.status,
            response.payload
        );

        // Check status from frame header
        let status = Status::try_from(response.status).unwrap_or(Status::Error);
        if status != Status::Success {
            return Err(ProtocolError::DeviceError(status));
        }

        // Response payload format: payload_len(2) + param_id(1) + value(N)
        if response.payload.len() < 3 {
            return Err(ProtocolError::InvalidFrame(
                "Parameter response too short".to_string(),
            ));
        }

        // Skip payload_len(2) + param_id(1) = 3 bytes to get value
        Ok(response.payload[3..].to_vec())
    }

    /// Request APS data indication (fetch waiting APS data)
    pub async fn request_aps_data(&self) -> Result<Vec<u8>, ProtocolError> {
        // APS_DATA_INDICATION request format: payload_len(2) + flags(1)
        // flags: 0x04 = request data
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u16.to_le_bytes()); // payload_len = 1
        payload.push(0x04); // flags: request data

        let response = self.request(CommandId::ApsDataIndication, payload).await?;

        tracing::info!(
            "ApsDataIndication response: status={}, payload={:02X?}",
            response.status,
            response.payload
        );

        // Check status
        let status = Status::try_from(response.status).unwrap_or(Status::Error);
        if status != Status::Success {
            return Err(ProtocolError::DeviceError(status));
        }

        // Parse and emit event for device announcements
        if let Ok(indication) = ApsDataIndication::parse(&response.payload) {
            tracing::info!(
                "Parsed APS Indication: cluster={:#06x} profile={:#06x} src_short={:#06x}",
                indication.cluster_id,
                indication.profile_id,
                indication.src_short_addr
            );

            // Check for device announcement (ZDO cluster 0x0013)
            if indication.profile_id == 0x0000 && indication.cluster_id == 0x0013 {
                if let Ok(announce) = DeviceAnnouncement::parse(&indication.asdu) {
                    let ieee_str = ApsDataIndication::format_ieee(&announce.ieee_addr);
                    tracing::info!(
                        "Device Announced: IEEE={} short={:#06x} cap={:#04x} router={} mains={}",
                        ieee_str,
                        announce.short_addr,
                        announce.capability,
                        announce.is_router(),
                        announce.is_mains_powered()
                    );
                    let _ = self.event_tx.send(DeconzEvent::DeviceAnnounced {
                        ieee_addr: announce.ieee_addr,
                        short_addr: announce.short_addr,
                        capability: announce.capability,
                    });
                }
            }

            let _ = self.event_tx.send(DeconzEvent::ApsIndication(indication));
        }

        Ok(response.payload)
    }

    /// Send APS data request (send command to a device)
    pub async fn send_aps_request(&self, request: ApsDataRequest) -> Result<(), ProtocolError> {
        let payload = request.serialize();

        tracing::debug!(
            "Sending APS request to {:#06x}:{} cluster={:#06x}",
            request.dest_short_addr,
            request.dest_endpoint,
            request.cluster_id
        );

        let response = self.request(CommandId::ApsDataRequest, payload).await?;

        tracing::debug!(
            "ApsDataRequest response: status={}, payload={:02X?}",
            response.status,
            response.payload
        );

        // Check status
        let status = Status::try_from(response.status).unwrap_or(Status::Error);
        if status != Status::Success {
            return Err(ProtocolError::DeviceError(status));
        }

        Ok(())
    }

    /// Write a network parameter
    pub async fn write_parameter(
        &self,
        param: NetworkParameter,
        value: &[u8],
    ) -> Result<(), ProtocolError> {
        // Request format per deCONZ Serial Protocol PDF:
        // payload_len(2 LE) + param_id(1) + value(N)
        // Where payload_len = 1 + len(value) (param_id + value bytes)
        let payload_len = (1 + value.len()) as u16;

        let mut payload = Vec::new();
        payload.extend_from_slice(&payload_len.to_le_bytes());
        payload.push(param as u8);
        payload.extend_from_slice(value);

        let response = self.request(CommandId::WriteParameter, payload).await?;

        tracing::debug!(
            "WriteParameter({:?}) response: status={}, payload={:02X?}",
            param,
            response.status,
            response.payload
        );

        // Check status from frame header
        let status = Status::try_from(response.status).unwrap_or(Status::Error);
        if status != Status::Success {
            return Err(ProtocolError::DeviceError(status));
        }

        Ok(())
    }
}

impl Drop for DeconzTransport {
    fn drop(&mut self) {
        // Signal shutdown (best effort)
        let _ = self.write_tx.try_send(WriteCommand::Shutdown);
    }
}
