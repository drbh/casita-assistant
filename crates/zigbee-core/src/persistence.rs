//! Device persistence using JSON file storage

use crate::device::ZigbeeDevice;
use std::path::Path;
use tokio::fs;

/// Load devices from a JSON file
pub async fn load_devices(path: &Path) -> Vec<ZigbeeDevice> {
    match fs::read_to_string(path).await {
        Ok(contents) => match serde_json::from_str::<Vec<ZigbeeDevice>>(&contents) {
            Ok(devices) => {
                tracing::info!("Loaded {} devices from {:?}", devices.len(), path);
                devices
            }
            Err(e) => {
                tracing::warn!("Failed to parse devices file {:?}: {}", path, e);
                Vec::new()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!("No devices file found at {:?}, starting fresh", path);
            Vec::new()
        }
        Err(e) => {
            tracing::warn!("Failed to read devices file {:?}: {}", path, e);
            Vec::new()
        }
    }
}

/// Save devices to a JSON file atomically
#[allow(clippy::missing_errors_doc)]
pub async fn save_devices(path: &Path, devices: &[ZigbeeDevice]) -> Result<(), std::io::Error> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Serialize to pretty JSON
    let json = serde_json::to_string_pretty(devices)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Write atomically: write to temp file, then rename
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json).await?;
    fs::rename(&tmp_path, path).await?;

    tracing::debug!("Saved {} devices to {:?}", devices.len(), path);
    Ok(())
}
