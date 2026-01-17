use bytes::{BufMut, Bytes, BytesMut};
use futures::StreamExt;
use retina::client::{Credentials, SessionGroup, SetupOptions};
use retina::codec::CodecItem;
use std::sync::Arc;
use tokio::sync::broadcast;
use url::Url;

#[derive(Clone, Debug)]
pub struct H264Parameters {
    /// `AvcDecoderConfig` (avcC box contents) - contains SPS/PPS
    pub avcc: Bytes,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct FrameData {
    /// NAL units for this frame (AVCC format: length-prefixed)
    pub data: Bytes,
    /// Whether this is a keyframe (IDR)
    pub is_keyframe: bool,
    /// New parameters if they changed (for init segment)
    pub new_parameters: Option<H264Parameters>,
}

pub struct RtspClient {
    url: Url,
    credentials: Option<Credentials>,
    session_group: Arc<SessionGroup>,
}

impl RtspClient {
    pub fn new(url: Url, username: Option<String>, password: Option<String>) -> Self {
        let credentials = match (username, password) {
            (Some(u), Some(p)) => Some(Credentials {
                username: u,
                password: p,
            }),
            _ => None,
        };

        Self {
            url,
            credentials,
            session_group: Arc::new(SessionGroup::default()),
        }
    }

    /// Returns a broadcast receiver for frames (parameters come with first frame that has them)
    pub async fn connect(&self) -> anyhow::Result<broadcast::Receiver<FrameData>> {
        let (tx, rx) = broadcast::channel(256); // ~8.5 seconds at 30fps

        let url = self.url.clone();
        let credentials = self.credentials.clone();
        let session_group = self.session_group.clone();

        tokio::spawn(async move {
            loop {
                match Self::run_stream(
                    url.clone(),
                    credentials.clone(),
                    session_group.clone(),
                    tx.clone(),
                )
                .await
                {
                    Ok(()) => {
                        tracing::info!("RTSP stream ended normally");
                        break;
                    }
                    Err(e) => {
                        if tx.receiver_count() == 0 {
                            tracing::info!("No more receivers, stopping RTSP stream");
                            break;
                        }

                        let err_str = e.to_string();
                        // Check if this is a keepalive-related error (Bad Request from GET_PARAMETER)
                        // Tapo cameras advertise GET_PARAMETER but respond with Bad Request
                        if err_str.contains("Bad Request") || err_str.contains("framing error") {
                            tracing::debug!("RTSP stream reconnecting (keepalive timeout)");
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            continue;
                        }
                        tracing::error!("RTSP stream error: {}", e);
                        break;
                    }
                }
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        Ok(rx)
    }

    #[allow(clippy::too_many_lines)] // RTSP streaming requires handling multiple protocol stages
    async fn run_stream(
        url: Url,
        credentials: Option<Credentials>,
        session_group: Arc<SessionGroup>,
        tx: broadcast::Sender<FrameData>,
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Connecting to RTSP stream: {}",
            url.host_str().unwrap_or("unknown")
        );

        let session_options = retina::client::SessionOptions::default()
            .session_group(session_group)
            .creds(credentials)
            .teardown(retina::client::TeardownPolicy::Never);

        let mut session = retina::client::Session::describe(url.clone(), session_options).await?;

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

        let mut session = session
            .play(retina::client::PlayOptions::default())
            .await?
            .demuxed()?;

        tracing::info!("RTSP session started");

        // Check if parameters are already available (out-of-band from SDP)
        let mut initial_params: Option<H264Parameters> = None;
        tracing::info!("Checking for video parameters in SDP...");
        if let Some(stream) = session.streams().get(video_stream_idx) {
            tracing::info!("Found video stream, checking parameters...");
            if let Some(params) = stream.parameters() {
                if let retina::codec::ParametersRef::Video(video_params) = params {
                    let extra_data = video_params.extra_data();
                    let (width, height) = video_params.pixel_dimensions();
                    tracing::info!(
                        "Got video parameters from SDP: {}x{}, extra_data len={}",
                        width,
                        height,
                        extra_data.len()
                    );
                    initial_params = Some(H264Parameters {
                        avcc: Bytes::copy_from_slice(extra_data),
                        width,
                        height,
                    });
                } else {
                    tracing::warn!("Parameters found but not video type");
                }
            } else {
                tracing::info!("No parameters available yet (will come in-band)");
            }
        } else {
            tracing::warn!("Video stream not found at index {}", video_stream_idx);
        }

        let mut sent_initial_params = false;
        let mut frame_count = 0u64;

        loop {
            match session.next().await {
                Some(Ok(item)) => {
                    if let CodecItem::VideoFrame(frame) = item {
                        frame_count += 1;
                        if frame_count <= 5 || frame_count % 100 == 0 {
                            tracing::info!(
                                "Frame {}: keyframe={}, data_len={}, has_new_params={}",
                                frame_count,
                                frame.is_random_access_point(),
                                frame.data().len(),
                                frame.has_new_parameters()
                            );
                        }
                        // Check if this frame has new parameters (in-band)
                        let new_parameters = if !sent_initial_params && initial_params.is_some() {
                            // Send the initial params we got from SDP
                            sent_initial_params = true;
                            initial_params.take()
                        } else if frame.has_new_parameters() {
                            if let Some(retina::codec::ParametersRef::Video(video_params)) = session
                                .streams()
                                .get(video_stream_idx)
                                .and_then(retina::client::Stream::parameters)
                            {
                                let extra_data = video_params.extra_data();
                                let (width, height) = video_params.pixel_dimensions();
                                tracing::info!(
                                    "Got updated video parameters: {}x{}, extra_data len={}",
                                    width,
                                    height,
                                    extra_data.len()
                                );
                                Some(H264Parameters {
                                    avcc: Bytes::copy_from_slice(extra_data),
                                    width,
                                    height,
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let frame_data = FrameData {
                            data: Bytes::copy_from_slice(frame.data()),
                            is_keyframe: frame.is_random_access_point(),
                            new_parameters,
                        };

                        if tx.send(frame_data).is_err() {
                            // No receivers, exit
                            break;
                        }
                    }
                }
                Some(Err(e)) => {
                    // Return the error so caller can decide to reconnect
                    return Err(anyhow::anyhow!("Stream error: {e}"));
                }
                None => {
                    tracing::info!("Stream ended");
                    break;
                }
            }
        }

        Ok(())
    }
}

pub struct Fmp4Writer {
    sequence_number: u32,
    base_decode_time: u64,
}

#[allow(clippy::cast_possible_truncation)] // MP4 box sizes are u32 per spec
impl Fmp4Writer {
    pub fn new() -> Self {
        Self {
            sequence_number: 1,
            base_decode_time: 0,
        }
    }

    pub fn write_init_segment(width: u32, height: u32, avcc: &[u8]) -> Bytes {
        let mut buf = BytesMut::with_capacity(512);

        // ftyp box
        Self::write_ftyp(&mut buf);

        // moov box
        Self::write_moov(&mut buf, width, height, avcc);

        buf.freeze()
    }

    pub fn write_media_segment(
        &mut self,
        frame_data: &[u8],
        is_keyframe: bool,
        duration: u32,
    ) -> Bytes {
        let mut buf = BytesMut::with_capacity(frame_data.len() + 256);

        let moof_start = buf.len();

        // moof box (we'll fix up data_offset after writing)
        Self::write_moof(
            &mut buf,
            self.sequence_number,
            self.base_decode_time,
            frame_data.len() as u32,
            duration,
            is_keyframe,
        );

        let moof_size = buf.len() - moof_start;

        // Fix up data_offset in trun box
        // data_offset = moof_size + 8 (mdat header)
        let data_offset = (moof_size + 8) as u32;
        // The data_offset is at a fixed position within moof:
        // moof header (8) + mfhd (16) + traf header (8) + tfhd (16) + tfdt (20) + trun header (12) + sample_count (4) = 84
        // So data_offset is at position moof_start + 84
        let data_offset_pos = moof_start + 84;
        buf[data_offset_pos..data_offset_pos + 4].copy_from_slice(&data_offset.to_be_bytes());

        // mdat box
        Self::write_mdat(&mut buf, frame_data);

        self.sequence_number += 1;
        self.base_decode_time += u64::from(duration);

        buf.freeze()
    }

    fn write_box_header(buf: &mut BytesMut, box_type: [u8; 4], size: u32) {
        buf.put_u32(size);
        buf.put_slice(&box_type);
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

    fn write_moov(buf: &mut BytesMut, width: u32, height: u32, avcc: &[u8]) {
        let start = buf.len();
        buf.put_u32(0); // placeholder
        buf.put_slice(b"moov");

        Self::write_mvhd(buf);
        Self::write_trak(buf, width, height, avcc);
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
        buf.put_u32(0x0001_0000); // rate (1.0)
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

    fn write_trak(buf: &mut BytesMut, width: u32, height: u32, avcc: &[u8]) {
        let start = buf.len();
        buf.put_u32(0); // placeholder
        buf.put_slice(b"trak");

        Self::write_tkhd(buf, width, height);
        Self::write_mdia(buf, width, height, avcc);

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

    fn write_mdia(buf: &mut BytesMut, width: u32, height: u32, avcc: &[u8]) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"mdia");

        Self::write_mdhd(buf);
        Self::write_hdlr(buf);
        Self::write_minf(buf, width, height, avcc);

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

    fn write_minf(buf: &mut BytesMut, width: u32, height: u32, avcc: &[u8]) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"minf");

        Self::write_vmhd(buf);
        Self::write_dinf(buf);
        Self::write_stbl(buf, width, height, avcc);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_vmhd(buf: &mut BytesMut) {
        Self::write_box_header(buf, *b"vmhd", 20);
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
        Self::write_box_header(buf, *b"url ", 12);
        buf.put_u8(0);
        buf.put_slice(&[0x00, 0x00, 0x01]); // flags = self-contained

        let dref_size = (buf.len() - dref_start) as u32;
        buf[dref_start..dref_start + 4].copy_from_slice(&dref_size.to_be_bytes());

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_stbl(buf: &mut BytesMut, width: u32, height: u32, avcc: &[u8]) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"stbl");

        Self::write_stsd(buf, width, height, avcc);
        Self::write_stts_empty(buf);
        Self::write_stsc_empty(buf);
        Self::write_stsz_empty(buf);
        Self::write_stco_empty(buf);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_stts_empty(buf: &mut BytesMut) {
        Self::write_box_header(buf, *b"stts", 16);
        buf.put_u8(0); // version
        buf.put_slice(&[0u8; 3]); // flags
        buf.put_u32(0); // entry_count
    }

    fn write_stsc_empty(buf: &mut BytesMut) {
        Self::write_box_header(buf, *b"stsc", 16);
        buf.put_u8(0); // version
        buf.put_slice(&[0u8; 3]); // flags
        buf.put_u32(0); // entry_count
    }

    fn write_stsz_empty(buf: &mut BytesMut) {
        Self::write_box_header(buf, *b"stsz", 20);
        buf.put_u8(0); // version
        buf.put_slice(&[0u8; 3]); // flags
        buf.put_u32(0); // sample_size (0 = sizes are in table)
        buf.put_u32(0); // sample_count
    }

    fn write_stco_empty(buf: &mut BytesMut) {
        Self::write_box_header(buf, *b"stco", 16);
        buf.put_u8(0); // version
        buf.put_slice(&[0u8; 3]); // flags
        buf.put_u32(0); // entry_count
    }

    fn write_stsd(buf: &mut BytesMut, width: u32, height: u32, avcc: &[u8]) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"stsd");
        buf.put_u8(0); // version
        buf.put_slice(&[0u8; 3]); // flags
        buf.put_u32(1); // entry_count

        Self::write_avc1(buf, width, height, avcc);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_avc1(buf: &mut BytesMut, width: u32, height: u32, avcc: &[u8]) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"avc1");
        buf.put_slice(&[0u8; 6]); // reserved
        buf.put_u16(1); // data_reference_index
        buf.put_slice(&[0u8; 16]); // pre_defined + reserved
        buf.put_u16(width as u16);
        buf.put_u16(height as u16);
        buf.put_u32(0x0048_0000); // horizresolution (72 dpi)
        buf.put_u32(0x0048_0000); // vertresolution (72 dpi)
        buf.put_u32(0); // reserved
        buf.put_u16(1); // frame_count
        buf.put_slice(&[0u8; 32]); // compressorname
        buf.put_u16(0x0018); // depth
        buf.put_i16(-1); // pre_defined

        // avcC box - write the raw avcC data from retina
        Self::write_avcc(buf, avcc);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_avcc(buf: &mut BytesMut, avcc_data: &[u8]) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"avcC");
        // Write the raw avcC data (already in correct format from retina)
        buf.put_slice(avcc_data);

        let size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }

    fn write_mvex(buf: &mut BytesMut) {
        let start = buf.len();
        buf.put_u32(0);
        buf.put_slice(b"mvex");

        // trex
        Self::write_box_header(buf, *b"trex", 32);
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
        Self::write_box_header(buf, *b"mfhd", 16);
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
        Self::write_box_header(buf, *b"tfhd", 16);
        buf.put_u8(0);
        buf.put_slice(&[0x02, 0x00, 0x00]); // flags: default-base-is-moof
        buf.put_u32(1); // track_ID

        // tfdt (track fragment decode time)
        Self::write_box_header(buf, *b"tfdt", 20);
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
                       // flags: 0x000305 = data-offset-present (0x01) + first-sample-flags-present (0x04) +
                       //                   sample-duration-present (0x100) + sample-size-present (0x200)
        buf.put_slice(&[0x00, 0x03, 0x05]);
        buf.put_u32(1); // sample_count

        // data_offset placeholder (will be fixed up by caller)
        buf.put_u32(0);

        // first_sample_flags
        let flags = if is_keyframe {
            0x0200_0000 // sample_depends_on = 2 (does not depend on others)
        } else {
            0x0101_0000 // sample_depends_on = 1 (depends on others), is_non_sync
        };
        buf.put_u32(flags);

        // Per-sample data (only duration and size, as indicated by flags)
        buf.put_u32(duration); // sample_duration
        buf.put_u32(data_size); // sample_size

        let trun_size = (buf.len() - start) as u32;
        buf[start..start + 4].copy_from_slice(&trun_size.to_be_bytes());
    }

    fn write_mdat(buf: &mut BytesMut, data: &[u8]) {
        Self::write_box_header(buf, *b"mdat", 8 + data.len() as u32);
        buf.put_slice(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmp4_init_segment() {
        // Minimal avcC (AvcDecoderConfig) for testing
        // Format: configVersion(1) + profile(1) + compat(1) + level(1) + lengthSize(1) + numSPS(1) + spsLen(2) + sps + numPPS(1) + ppsLen(2) + pps
        let avcc = vec![
            0x01, // configurationVersion
            0x64, // AVCProfileIndication (High)
            0x00, // profile_compatibility
            0x1f, // AVCLevelIndication (3.1)
            0xff, // lengthSizeMinusOne = 3 (4 bytes)
            0xe1, // numOfSequenceParameterSets = 1
            0x00, 0x04, // sps length = 4
            0x67, 0x64, 0x00, 0x1f, // sps data
            0x01, // numOfPictureParameterSets = 1
            0x00, 0x04, // pps length = 4
            0x68, 0xeb, 0xe3, 0xcb, // pps data
        ];
        let init = Fmp4Writer::write_init_segment(1920, 1080, &avcc);

        // Check ftyp box marker
        assert_eq!(&init[4..8], b"ftyp");
    }
}
