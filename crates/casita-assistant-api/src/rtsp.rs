//! Native RTSP stream handling using the retina crate
//!
//! This module provides efficient RTSP streaming without requiring ffmpeg.
//! It uses the retina crate for native Rust RTSP/RTP handling.

use bytes::{BufMut, Bytes, BytesMut};
use futures::StreamExt;
use retina::client::{SessionGroup, SetupOptions};
use retina::codec::{CodecItem, VideoFrame};
use std::sync::Arc;
use tokio::sync::broadcast;
use url::Url;

/// Build an RTSP URL with embedded credentials
pub fn build_rtsp_url(
    base_url: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> anyhow::Result<Url> {
    let mut url = Url::parse(base_url)?;

    if let Some(user) = username {
        url.set_username(user)
            .map_err(|_| anyhow::anyhow!("Failed to set username"))?;
    }
    if let Some(pass) = password {
        url.set_password(Some(pass))
            .map_err(|_| anyhow::anyhow!("Failed to set password"))?;
    }

    Ok(url)
}

/// H.264 stream parameters extracted from SPS/PPS
#[derive(Clone, Debug)]
pub struct H264Parameters {
    pub sps: Bytes,
    pub pps: Bytes,
    pub width: u32,
    pub height: u32,
}

/// Frame data for streaming
#[derive(Clone, Debug)]
pub struct FrameData {
    /// NAL units for this frame
    pub data: Bytes,
    /// Whether this is a keyframe (IDR)
    pub is_keyframe: bool,
    /// Presentation timestamp in 90kHz units
    pub pts: i64,
}

/// Native RTSP client using retina
pub struct RtspClient {
    url: Url,
    session_group: Arc<SessionGroup>,
}

impl RtspClient {
    /// Create a new RTSP client
    pub fn new(url: Url) -> Self {
        Self {
            url,
            session_group: Arc::new(SessionGroup::default()),
        }
    }

    /// Connect and start receiving frames
    /// Returns H.264 parameters and a broadcast receiver for frames
    pub async fn connect(
        &self,
    ) -> anyhow::Result<(H264Parameters, broadcast::Receiver<FrameData>)> {
        let (tx, rx) = broadcast::channel(16);

        let url = self.url.clone();
        let session_group = self.session_group.clone();

        // Spawn connection task
        tokio::spawn(async move {
            if let Err(e) = Self::run_stream(url, session_group, tx).await {
                tracing::error!("RTSP stream error: {}", e);
            }
        });

        // Wait a bit for initial connection and parameters
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Return placeholder parameters - actual params come with first keyframe
        let params = H264Parameters {
            sps: Bytes::new(),
            pps: Bytes::new(),
            width: 1920,
            height: 1080,
        };

        Ok((params, rx))
    }

    async fn run_stream(
        url: Url,
        session_group: Arc<SessionGroup>,
        tx: broadcast::Sender<FrameData>,
    ) -> anyhow::Result<()> {
        tracing::info!("Connecting to RTSP stream: {}", url.host_str().unwrap_or("unknown"));

        let mut session = retina::client::Session::describe(
            url.clone(),
            retina::client::SessionOptions::default()
                .session_group(session_group),
        )
        .await?;

        // Find and setup video stream
        let video_stream_idx = session
            .streams()
            .iter()
            .position(|s| s.media() == "video")
            .ok_or_else(|| anyhow::anyhow!("No video stream found"))?;

        session
            .setup(
                video_stream_idx,
                SetupOptions::default().transport(retina::client::Transport::Tcp(
                    retina::client::TcpTransportOptions::default(),
                )),
            )
            .await?;

        let mut session = session.play(retina::client::PlayOptions::default()).await?.demuxed()?;

        tracing::info!("RTSP session started");

        loop {
            match session.next().await {
                Some(Ok(item)) => {
                    if let CodecItem::VideoFrame(frame) = item {
                        let frame_data = Self::process_video_frame(&frame);
                        if tx.send(frame_data).is_err() {
                            // No receivers, exit
                            break;
                        }
                    }
                }
                Some(Err(e)) => {
                    tracing::error!("Stream error: {}", e);
                    break;
                }
                None => {
                    tracing::info!("Stream ended");
                    break;
                }
            }
        }

        Ok(())
    }

    fn process_video_frame(frame: &VideoFrame) -> FrameData {
        FrameData {
            data: Bytes::copy_from_slice(frame.data()),
            is_keyframe: frame.is_random_access_point(),
            pts: frame.timestamp().elapsed() as i64,
        }
    }
}

/// fMP4 (Fragmented MP4) writer for MSE streaming
/// This packages H.264 NAL units into fMP4 segments for browser playback
pub struct Fmp4Writer {
    sequence_number: u32,
    base_decode_time: u64,
}

impl Fmp4Writer {
    pub fn new() -> Self {
        Self {
            sequence_number: 1,
            base_decode_time: 0,
        }
    }

    /// Generate initialization segment (ftyp + moov)
    pub fn write_init_segment(&self, width: u32, height: u32, sps: &[u8], pps: &[u8]) -> Bytes {
        let mut buf = BytesMut::with_capacity(512);

        // ftyp box
        Self::write_ftyp(&mut buf);

        // moov box
        Self::write_moov(&mut buf, width, height, sps, pps);

        buf.freeze()
    }

    /// Generate a media segment (moof + mdat)
    pub fn write_media_segment(
        &mut self,
        frame_data: &[u8],
        is_keyframe: bool,
        duration: u32,
    ) -> Bytes {
        let mut buf = BytesMut::with_capacity(frame_data.len() + 256);

        // moof box
        Self::write_moof(
            &mut buf,
            self.sequence_number,
            self.base_decode_time,
            frame_data.len() as u32,
            duration,
            is_keyframe,
        );

        // mdat box
        Self::write_mdat(&mut buf, frame_data);

        self.sequence_number += 1;
        self.base_decode_time += duration as u64;

        buf.freeze()
    }

    fn write_box_header(buf: &mut BytesMut, box_type: &[u8; 4], size: u32) {
        buf.put_u32(size);
        buf.put_slice(box_type);
    }

    fn write_ftyp(buf: &mut BytesMut) {
        let start = buf.len();
        buf.put_u32(0); // placeholder for size
        buf.put_slice(b"ftyp");
        buf.put_slice(b"isom"); // major brand
        buf.put_u32(0x200); // minor version
        buf.put_slice(b"isom"); // compatible brands
        buf.put_slice(b"iso6");
        buf.put_slice(b"mp41");

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_moov(buf: &mut BytesMut, width: u32, height: u32, sps: &[u8], pps: &[u8]) {
        let start = buf.len();
        buf.put_u32(0); // placeholder
        buf.put_slice(b"moov");

        Self::write_mvhd(buf);
        Self::write_trak(buf, width, height, sps, pps);
        Self::write_mvex(buf);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_mvhd(buf: &mut BytesMut) {
        let start = buf.len();
        buf.put_u32(0); // placeholder
        buf.put_slice(b"mvhd");
        buf.put_u8(0); // version
        buf.put_slice(&[0u8; 3]); // flags
        buf.put_u32(0); // creation time
        buf.put_u32(0); // modification time
        buf.put_u32(90000); // timescale (90kHz for video)
        buf.put_u32(0); // duration
        buf.put_u32(0x00010000); // rate (1.0)
        buf.put_u16(0x0100); // volume (1.0)
        buf.put_slice(&[0u8; 10]); // reserved
        // identity matrix
        buf.put_slice(&[
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00,
        ]);
        buf.put_slice(&[0u8; 24]); // pre_defined
        buf.put_u32(2); // next_track_ID

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_trak(buf: &mut BytesMut, width: u32, height: u32, sps: &[u8], pps: &[u8]) {
        let start = buf.len();
        buf.put_u32(0); // placeholder
        buf.put_slice(b"trak");

        Self::write_tkhd(buf, width, height);
        Self::write_mdia(buf, width, height, sps, pps);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_tkhd(buf: &mut BytesMut, width: u32, height: u32) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"tkhd");
        buf.put_u8(0); // version
        buf.put_slice(&[0x00, 0x00, 0x03]); // flags (track enabled, in movie)
        buf.put_u32(0); // creation_time
        buf.put_u32(0); // modification_time
        buf.put_u32(1); // track_ID
        buf.put_u32(0); // reserved
        buf.put_u32(0); // duration
        buf.put_slice(&[0u8; 8]); // reserved
        buf.put_u16(0); // layer
        buf.put_u16(0); // alternate_group
        buf.put_u16(0); // volume
        buf.put_u16(0); // reserved
        // identity matrix
        buf.put_slice(&[
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00,
        ]);
        buf.put_u32(width << 16); // width (16.16 fixed point)
        buf.put_u32(height << 16); // height

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_mdia(buf: &mut BytesMut, width: u32, height: u32, sps: &[u8], pps: &[u8]) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"mdia");

        Self::write_mdhd(buf);
        Self::write_hdlr(buf);
        Self::write_minf(buf, width, height, sps, pps);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_mdhd(buf: &mut BytesMut) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"mdhd");
        buf.put_u8(0); // version
        buf.put_slice(&[0u8; 3]); // flags
        buf.put_u32(0); // creation_time
        buf.put_u32(0); // modification_time
        buf.put_u32(90000); // timescale
        buf.put_u32(0); // duration
        buf.put_u16(0x55c4); // language (und)
        buf.put_u16(0); // pre_defined

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_hdlr(buf: &mut BytesMut) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"hdlr");
        buf.put_u8(0); // version
        buf.put_slice(&[0u8; 3]); // flags
        buf.put_u32(0); // pre_defined
        buf.put_slice(b"vide"); // handler_type
        buf.put_slice(&[0u8; 12]); // reserved
        buf.put_slice(b"VideoHandler\0"); // name

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_minf(buf: &mut BytesMut, width: u32, height: u32, sps: &[u8], pps: &[u8]) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"minf");

        Self::write_vmhd(buf);
        Self::write_dinf(buf);
        Self::write_stbl(buf, width, height, sps, pps);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_vmhd(buf: &mut BytesMut) {
        Self::write_box_header(buf, b"vmhd", 20);
        buf.put_u8(0); // version
        buf.put_slice(&[0x00, 0x00, 0x01]); // flags
        buf.put_u16(0); // graphicsmode
        buf.put_slice(&[0u8; 6]); // opcolor
    }

    fn write_dinf(buf: &mut BytesMut) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"dinf");

        // dref
        let dref_start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"dref");
        buf.put_u8(0); // version
        buf.put_slice(&[0u8; 3]); // flags
        buf.put_u32(1); // entry_count

        // url (self-reference)
        Self::write_box_header(buf, b"url ", 12);
        buf.put_u8(0);
        buf.put_slice(&[0x00, 0x00, 0x01]); // flags = self-contained

        let dref_size = (buf.len() - dref_start) as u32;
        buf[dref_start..dref_start + 4].copy_from_slice(&dref_size.to_be_bytes());

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_stbl(buf: &mut BytesMut, width: u32, height: u32, sps: &[u8], pps: &[u8]) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"stbl");

        Self::write_stsd(buf, width, height, sps, pps);
        Self::write_empty_box(buf, b"stts");
        Self::write_empty_box(buf, b"stsc");
        Self::write_empty_box(buf, b"stsz");
        Self::write_empty_box(buf, b"stco");

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_empty_box(buf: &mut BytesMut, box_type: &[u8; 4]) {
        Self::write_box_header(buf, box_type, 16);
        buf.put_u8(0); // version
        buf.put_slice(&[0u8; 3]); // flags
        buf.put_u32(0); // entry_count
    }

    fn write_stsd(buf: &mut BytesMut, width: u32, height: u32, sps: &[u8], pps: &[u8]) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"stsd");
        buf.put_u8(0); // version
        buf.put_slice(&[0u8; 3]); // flags
        buf.put_u32(1); // entry_count

        Self::write_avc1(buf, width, height, sps, pps);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_avc1(buf: &mut BytesMut, width: u32, height: u32, sps: &[u8], pps: &[u8]) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"avc1");
        buf.put_slice(&[0u8; 6]); // reserved
        buf.put_u16(1); // data_reference_index
        buf.put_slice(&[0u8; 16]); // pre_defined + reserved
        buf.put_u16(width as u16);
        buf.put_u16(height as u16);
        buf.put_u32(0x00480000); // horizresolution (72 dpi)
        buf.put_u32(0x00480000); // vertresolution (72 dpi)
        buf.put_u32(0); // reserved
        buf.put_u16(1); // frame_count
        buf.put_slice(&[0u8; 32]); // compressorname
        buf.put_u16(0x0018); // depth
        buf.put_i16(-1); // pre_defined

        Self::write_avcc(buf, sps, pps);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_avcc(buf: &mut BytesMut, sps: &[u8], pps: &[u8]) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"avcC");
        buf.put_u8(1); // configurationVersion
        buf.put_u8(if sps.len() > 1 { sps[1] } else { 0x64 }); // AVCProfileIndication
        buf.put_u8(if sps.len() > 2 { sps[2] } else { 0x00 }); // profile_compatibility
        buf.put_u8(if sps.len() > 3 { sps[3] } else { 0x1f }); // AVCLevelIndication
        buf.put_u8(0xff); // 6 bits reserved + 2 bits lengthSizeMinusOne (3)
        buf.put_u8(0xe1); // 3 bits reserved + 5 bits numOfSequenceParameterSets (1)
        buf.put_u16(sps.len() as u16);
        buf.put_slice(sps);
        buf.put_u8(1); // numOfPictureParameterSets
        buf.put_u16(pps.len() as u16);
        buf.put_slice(pps);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_mvex(buf: &mut BytesMut) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"mvex");

        // trex
        Self::write_box_header(buf, b"trex", 32);
        buf.put_u8(0); // version
        buf.put_slice(&[0u8; 3]); // flags
        buf.put_u32(1); // track_ID
        buf.put_u32(1); // default_sample_description_index
        buf.put_u32(0); // default_sample_duration
        buf.put_u32(0); // default_sample_size
        buf.put_u32(0); // default_sample_flags

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_moof(
        buf: &mut BytesMut,
        sequence_number: u32,
        base_decode_time: u64,
        data_size: u32,
        duration: u32,
        is_keyframe: bool,
    ) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"moof");

        // mfhd
        Self::write_box_header(buf, b"mfhd", 16);
        buf.put_u8(0);
        buf.put_slice(&[0u8; 3]);
        buf.put_u32(sequence_number);

        // traf
        Self::write_traf(buf, base_decode_time, data_size, duration, is_keyframe);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_traf(
        buf: &mut BytesMut,
        base_decode_time: u64,
        data_size: u32,
        duration: u32,
        is_keyframe: bool,
    ) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"traf");

        // tfhd
        Self::write_box_header(buf, b"tfhd", 16);
        buf.put_u8(0);
        buf.put_slice(&[0x02, 0x00, 0x00]); // flags: default-base-is-moof
        buf.put_u32(1); // track_ID

        // tfdt (track fragment decode time)
        Self::write_box_header(buf, b"tfdt", 20);
        buf.put_u8(1); // version 1 for 64-bit time
        buf.put_slice(&[0u8; 3]);
        buf.put_u64(base_decode_time);

        // trun
        Self::write_trun(buf, data_size, duration, is_keyframe);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_trun(buf: &mut BytesMut, data_size: u32, duration: u32, is_keyframe: bool) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"trun");
        buf.put_u8(0); // version
        // flags: data-offset-present, first-sample-flags-present, sample-duration-present, sample-size-present
        buf.put_slice(&[0x00, 0x0f, 0x01]);
        buf.put_u32(1); // sample_count

        // Calculate data_offset (offset from start of moof to mdat data)
        // This will be updated after we know the moof size
        let _data_offset_pos = buf.len();
        buf.put_u32(0); // placeholder for data_offset

        // first_sample_flags
        let flags = if is_keyframe {
            0x02000000 // sample_depends_on = 2 (does not depend on others)
        } else {
            0x01010000 // sample_depends_on = 1 (depends on others), is_non_sync
        };
        buf.put_u32(flags);

        buf.put_u32(duration); // sample_duration
        buf.put_u32(data_size); // sample_size

        let trun_size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&trun_size.to_be_bytes());

        // Calculate and write data_offset (moof size + mdat header = trun pos + remaining + 8)
        // Actually, we need to write the offset from moof start to mdat data
        // This is computed as: size of entire moof + 8 (mdat header)
        // We'll fix this in the caller
    }

    fn write_mdat(buf: &mut BytesMut, data: &[u8]) {
        Self::write_box_header(buf, b"mdat", 8 + data.len() as u32);
        buf.put_slice(data);
    }
}

/// MJPEG frame from RTSP
/// For cameras that natively output MJPEG, we can pass through directly
pub struct MjpegFrame {
    pub data: Bytes,
    pub timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_rtsp_url_with_credentials() {
        let url = build_rtsp_url(
            "rtsp://192.168.1.166:554/stream1",
            Some("user"),
            Some("pass"),
        )
        .unwrap();
        assert_eq!(url.as_str(), "rtsp://user:pass@192.168.1.166:554/stream1");
    }

    #[test]
    fn test_build_rtsp_url_without_credentials() {
        let url = build_rtsp_url("rtsp://192.168.1.166:554/stream1", None, None).unwrap();
        assert_eq!(url.as_str(), "rtsp://192.168.1.166:554/stream1");
    }

    #[test]
    fn test_fmp4_init_segment() {
        let writer = Fmp4Writer::new();
        // Minimal SPS/PPS for testing
        let sps = vec![0x67, 0x64, 0x00, 0x1f];
        let pps = vec![0x68, 0xeb, 0xe3, 0xcb];
        let init = writer.write_init_segment(1920, 1080, &sps, &pps);

        // Check ftyp box marker
        assert_eq!(&init[4..8], b"ftyp");
    }
}
