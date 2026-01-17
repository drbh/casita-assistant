//! deCONZ frame structure and CRC handling

use crate::commands::CommandId;
use crate::types::ProtocolError;

/// Minimum frame size: cmd(1) + seq(1) + status(1) + `frame_len(2)` + crc(2) = 7
pub const MIN_FRAME_SIZE: usize = 7;

/// deCONZ protocol frame
///
/// Frame format:
/// ```text
/// [Command ID: 1 byte]
/// [Sequence: 1 byte]
/// [Status: 1 byte] (0=success in responses, 0=reserved in requests)
/// [Frame Length: 2 bytes LE] (frame size NOT including CRC)
/// [Payload: variable]
/// [CRC: 2 bytes LE]
/// ```
#[derive(Debug, Clone)]
pub struct Frame {
    pub command_id: CommandId,
    pub sequence: u8,
    pub status: u8,
    pub payload: Vec<u8>,
}

impl Frame {
    /// Create a new frame (for requests, status=0)
    #[must_use]
    pub fn new(command_id: CommandId, sequence: u8, payload: Vec<u8>) -> Self {
        Self {
            command_id,
            sequence,
            status: 0,
            payload,
        }
    }

    /// Serialize frame to bytes (ready for SLIP encoding)
    #[must_use]
    #[allow(clippy::missing_panics_doc)] // Panic only on protocol-violating payload size
    pub fn serialize(&self) -> Vec<u8> {
        // frame_len = cmd(1) + seq(1) + status(1) + frame_len(2) + payload
        // Note: frame_len does NOT include the CRC bytes
        let frame_len =
            u16::try_from(5 + self.payload.len()).expect("payload exceeds protocol maximum");

        let mut data = Vec::with_capacity(frame_len as usize + 2); // +2 for CRC

        // Header
        data.push(self.command_id as u8);
        data.push(self.sequence);
        data.push(0x00); // Reserved/Status

        // Frame length (LE) - does NOT include CRC
        data.extend_from_slice(&frame_len.to_le_bytes());

        // Payload (includes its own length prefix for commands that need it)
        data.extend_from_slice(&self.payload);

        // Calculate and append CRC
        let crc = Self::calculate_crc(&data);
        data.extend_from_slice(&crc.to_le_bytes());

        data
    }

    /// Deserialize frame from bytes (after SLIP decoding)
    #[allow(clippy::missing_errors_doc)]
    pub fn deserialize(data: &[u8]) -> Result<Self, ProtocolError> {
        if data.len() < MIN_FRAME_SIZE {
            return Err(ProtocolError::FrameTooShort(data.len()));
        }

        // Verify CRC first
        let crc_offset = data.len() - 2;
        let received_crc = u16::from_le_bytes([data[crc_offset], data[crc_offset + 1]]);
        let calculated_crc = Self::calculate_crc(&data[..crc_offset]);

        if received_crc != calculated_crc {
            return Err(ProtocolError::CrcMismatch {
                expected: calculated_crc,
                actual: received_crc,
            });
        }

        // Parse header
        let command_id =
            CommandId::from_u8(data[0]).ok_or_else(|| ProtocolError::UnknownCommand(data[0]))?;
        let sequence = data[1];
        let status = data[2]; // Status in responses, reserved (0) in requests

        // Frame length (for validation) - does NOT include CRC
        let frame_len = u16::from_le_bytes([data[3], data[4]]) as usize;
        let expected_total = frame_len + 2; // +2 for CRC
        if expected_total != data.len() {
            return Err(ProtocolError::InvalidFrame(format!(
                "Frame length mismatch: header says {} (+2 CRC = {}), actual {}",
                frame_len,
                expected_total,
                data.len()
            )));
        }

        // For responses, the payload is everything after the header until CRC
        // The header is: cmd(1) + seq(1) + status(1) + frame_len(2) = 5 bytes
        let payload_start = 5;
        let payload = data[payload_start..crc_offset].to_vec();

        Ok(Self {
            command_id,
            sequence,
            status,
            payload,
        })
    }

    /// Calculate 16-bit CRC (two's complement of sum)
    #[must_use]
    pub fn calculate_crc(data: &[u8]) -> u16 {
        let sum: u16 = data.iter().map(|&b| u16::from(b)).sum();
        (!sum).wrapping_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc_calculation() {
        let data = vec![0x0D, 0x01, 0x00, 0x09, 0x00, 0x00, 0x00];
        let crc = Frame::calculate_crc(&data);

        // Verify CRC by checking the calculated value
        let sum: u16 = data.iter().map(|&b| u16::from(b)).sum();
        let expected_crc = (!sum).wrapping_add(1);
        assert_eq!(crc, expected_crc);
    }

    #[test]
    fn test_frame_too_short() {
        let result = Frame::deserialize(&[0x01, 0x02]);
        assert!(matches!(result, Err(ProtocolError::FrameTooShort(_))));
    }
}
