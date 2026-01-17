//! Automation persistence using JSON file storage

use crate::model::Automation;
use std::path::Path;
use tokio::fs;

/// Load automations from a JSON file
pub async fn load_automations(path: &Path) -> Vec<Automation> {
    match fs::read_to_string(path).await {
        Ok(contents) => match serde_json::from_str::<Vec<Automation>>(&contents) {
            Ok(automations) => {
                tracing::info!("Loaded {} automations from {:?}", automations.len(), path);
                automations
            }
            Err(e) => {
                tracing::warn!("Failed to parse automations file {:?}: {}", path, e);
                Vec::new()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!("No automations file found at {:?}, starting fresh", path);
            Vec::new()
        }
        Err(e) => {
            tracing::warn!("Failed to read automations file {:?}: {}", path, e);
            Vec::new()
        }
    }
}

/// Save automations to a JSON file atomically
#[allow(clippy::missing_errors_doc)]
pub async fn save_automations(
    path: &Path,
    automations: &[Automation],
) -> Result<(), std::io::Error> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Serialize to pretty JSON
    let json = serde_json::to_string_pretty(automations)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Write atomically: write to temp file, then rename
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json).await?;
    fs::rename(&tmp_path, path).await?;

    tracing::debug!("Saved {} automations to {:?}", automations.len(), path);
    Ok(())
}
