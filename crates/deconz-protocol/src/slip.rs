//! SLIP (Serial Line Internet Protocol) framing - RFC 1055
//!
//! SLIP is used to frame binary data over serial connections.

/// SLIP END byte - marks frame boundaries
pub const SLIP_END: u8 = 0xC0;
/// SLIP ESC byte - escape character
pub const SLIP_ESC: u8 = 0xDB;
/// SLIP ESC_END - escaped form of END
pub const SLIP_ESC_END: u8 = 0xDC;
/// SLIP ESC_ESC - escaped form of ESC
pub const SLIP_ESC_ESC: u8 = 0xDD;

/// SLIP encoder for outgoing frames
pub struct SlipEncoder;

impl SlipEncoder {
    /// Encode data with SLIP framing
    ///
    /// Prefixes and suffixes with END byte (Phil Karn's improvement)
    /// to flush any line noise before the actual frame.
    pub fn encode(data: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(data.len() * 2 + 2);
        encoded.push(SLIP_END); // Start with END to flush noise

        for &byte in data {
            match byte {
                SLIP_END => {
                    encoded.push(SLIP_ESC);
                    encoded.push(SLIP_ESC_END);
                }
                SLIP_ESC => {
                    encoded.push(SLIP_ESC);
                    encoded.push(SLIP_ESC_ESC);
                }
                _ => encoded.push(byte),
            }
        }

        encoded.push(SLIP_END);
        encoded
    }
}

/// SLIP decoder for incoming frames
pub struct SlipDecoder {
    buffer: Vec<u8>,
    in_escape: bool,
}

impl Default for SlipDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl SlipDecoder {
    /// Create a new SLIP decoder
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(256),
            in_escape: false,
        }
    }

    /// Feed bytes into the decoder and extract complete frames
    ///
    /// Returns a vector of complete frames (may be empty if no complete frames yet)
    pub fn feed(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();

        for &byte in data {
            if self.in_escape {
                self.in_escape = false;
                match byte {
                    SLIP_ESC_END => self.buffer.push(SLIP_END),
                    SLIP_ESC_ESC => self.buffer.push(SLIP_ESC),
                    // Invalid escape sequence - push as-is
                    _ => {
                        self.buffer.push(SLIP_ESC);
                        self.buffer.push(byte);
                    }
                }
            } else {
                match byte {
                    SLIP_END => {
                        if !self.buffer.is_empty() {
                            frames.push(std::mem::take(&mut self.buffer));
                        }
                    }
                    SLIP_ESC => {
                        self.in_escape = true;
                    }
                    _ => {
                        self.buffer.push(byte);
                    }
                }
            }
        }

        frames
    }

    /// Clear the decoder state
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.in_escape = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_simple() {
        let data = vec![0x01, 0x02, 0x03];
        let encoded = SlipEncoder::encode(&data);
        assert_eq!(encoded, vec![SLIP_END, 0x01, 0x02, 0x03, SLIP_END]);
    }

    #[test]
    fn test_encode_with_end_byte() {
        let data = vec![0x01, SLIP_END, 0x03];
        let encoded = SlipEncoder::encode(&data);
        assert_eq!(
            encoded,
            vec![SLIP_END, 0x01, SLIP_ESC, SLIP_ESC_END, 0x03, SLIP_END]
        );
    }

    #[test]
    fn test_encode_with_esc_byte() {
        let data = vec![0x01, SLIP_ESC, 0x03];
        let encoded = SlipEncoder::encode(&data);
        assert_eq!(
            encoded,
            vec![SLIP_END, 0x01, SLIP_ESC, SLIP_ESC_ESC, 0x03, SLIP_END]
        );
    }

    #[test]
    fn test_decode_simple() {
        let mut decoder = SlipDecoder::new();
        let frames = decoder.feed(&[SLIP_END, 0x01, 0x02, 0x03, SLIP_END]);
        assert_eq!(frames, vec![vec![0x01, 0x02, 0x03]]);
    }

    #[test]
    fn test_decode_with_escapes() {
        let mut decoder = SlipDecoder::new();
        let frames = decoder.feed(&[
            SLIP_END,
            0x01,
            SLIP_ESC,
            SLIP_ESC_END,
            SLIP_ESC,
            SLIP_ESC_ESC,
            SLIP_END,
        ]);
        assert_eq!(frames, vec![vec![0x01, SLIP_END, SLIP_ESC]]);
    }

    #[test]
    fn test_decode_partial() {
        let mut decoder = SlipDecoder::new();

        // First part
        let frames = decoder.feed(&[SLIP_END, 0x01, 0x02]);
        assert!(frames.is_empty());

        // Second part
        let frames = decoder.feed(&[0x03, SLIP_END]);
        assert_eq!(frames, vec![vec![0x01, 0x02, 0x03]]);
    }

    #[test]
    fn test_roundtrip() {
        let original = vec![0x01, SLIP_END, 0x02, SLIP_ESC, 0x03, 0x00, 0xFF];
        let encoded = SlipEncoder::encode(&original);
        let mut decoder = SlipDecoder::new();
        let decoded = decoder.feed(&encoded);
        assert_eq!(decoded, vec![original]);
    }
}
