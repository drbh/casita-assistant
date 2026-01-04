//! RTSP stream transcoding via FFmpeg subprocess

use std::process::Stdio;
use tokio::io::AsyncBufReadExt;
use tokio::process::ChildStdout;

/// FFmpeg-based RTSP to MJPEG transcoder
pub struct RtspTranscoder {
    process: Option<tokio::process::Child>,
}

impl RtspTranscoder {
    /// Create a new RTSP transcoder
    pub fn new() -> Self {
        Self { process: None }
    }

    /// Build the RTSP URL with embedded credentials
    pub fn build_rtsp_url(
        base_url: &str,
        username: Option<&str>,
        password: Option<&str>,
    ) -> String {
        match (username, password) {
            (Some(user), Some(pass)) => {
                // Insert credentials into URL: rtsp://user:pass@host:port/path
                if base_url.starts_with("rtsp://") {
                    let rest = &base_url[7..]; // Skip "rtsp://"
                    format!("rtsp://{}:{}@{}", user, pass, rest)
                } else {
                    base_url.to_string()
                }
            }
            _ => base_url.to_string(),
        }
    }

    /// Start FFmpeg transcoding process
    /// Returns the stdout which streams MJPEG data
    pub async fn start(&mut self, rtsp_url: &str) -> Result<ChildStdout, std::io::Error> {
        // Kill any existing process
        self.stop().await;

        tracing::info!("Starting FFmpeg transcoder for RTSP stream");

        // Build FFmpeg command
        let mut cmd = tokio::process::Command::new("ffmpeg");
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-rtsp_transport",
            "tcp", // Use TCP for reliability
            "-i",
            rtsp_url, // Input RTSP stream
            "-an",    // No audio
            "-c:v",
            "mjpeg", // Output codec: MJPEG
            "-q:v",
            "5", // Quality (2-31, lower=better)
            "-f",
            "mjpeg", // Output format
            "-r",
            "15", // 15 fps
            "-",  // Output to stdout
        ]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn()?;

        // Spawn task to log stderr
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!("FFmpeg: {}", line);
                }
            });
        }

        let stdout = child.stdout.take().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "Failed to capture FFmpeg stdout")
        })?;

        self.process = Some(child);
        Ok(stdout)
    }

    /// Stop the transcoder
    pub async fn stop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill().await;
            let _ = process.wait().await;
            tracing::debug!("FFmpeg process stopped");
        }
    }
}

impl Drop for RtspTranscoder {
    fn drop(&mut self) {
        // Best effort cleanup - can't await in Drop
        if let Some(mut process) = self.process.take() {
            let _ = process.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_rtsp_url_with_credentials() {
        let url = RtspTranscoder::build_rtsp_url(
            "rtsp://192.168.1.166:554/stream1",
            Some("user"),
            Some("pass"),
        );
        assert_eq!(url, "rtsp://user:pass@192.168.1.166:554/stream1");
    }

    #[test]
    fn test_build_rtsp_url_without_credentials() {
        let url = RtspTranscoder::build_rtsp_url("rtsp://192.168.1.166:554/stream1", None, None);
        assert_eq!(url, "rtsp://192.168.1.166:554/stream1");
    }
}
