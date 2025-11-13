use anyhow::{Context, Result};
use std::fs;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use crate::settings::config::AppSettings;

/// Returns the directory where user configurations are stored.
pub fn get_configs_dir() -> Result<PathBuf> {
    let user_dirs = directories::UserDirs::new().context("Could not get user directories")?;
    let docs_dir = user_dirs.document_dir().context("Could not find the Documents folder")?;
    let configs_dir = docs_dir.join("LABHConfig").join("configs");
    fs::create_dir_all(&configs_dir)
        .with_context(|| format!("Failed to create configs directory at {}", configs_dir.display()))?;
    Ok(configs_dir)
}

/// Lists all valid .yaml/.yml config files in the configs directory.
pub fn list_configs() -> Result<Vec<String>> {
    let configs_dir = get_configs_dir()?;
    let mut configs = Vec::new();

    for entry in fs::read_dir(configs_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if ext == "yaml" || ext == "yml" {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        configs.push(stem.to_string());
                    }
                }
            }
        }
    }
    configs.sort();
    Ok(configs)
}

/// Loads an AppSettings configuration from a given file name.
pub fn load_config(name: &str) -> Result<AppSettings> {
    let path = get_configs_dir()?.join(format!("{}.yaml", name));
    let file = fs::File::open(&path)
        .with_context(|| format!("Failed to open config file at {}", path.display()))?;
    let reader = BufReader::new(file);
    let settings: AppSettings = serde_yaml::from_reader(reader)
        .with_context(|| format!("Failed to parse config file {}", name))?;
    log::info!("Loaded config '{}' from {}", name, path.display());
    Ok(settings)
}

/// Saves the current AppSettings to a file with the given name.
pub fn save_config(name: &str, settings: &AppSettings) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("Config name cannot be empty.");
    }
    let path = get_configs_dir()?.join(format!("{}.yaml", name));
    let file = fs::File::options().create(true).truncate(true).write(true).open(&path)
        .with_context(|| format!("Failed to create or open config file at {}", path.display()))?;
    let writer = BufWriter::new(file);
    serde_yaml::to_writer(writer, settings)
        .with_context(|| format!("Failed to serialize and save config {}", name))?;
    log::info!("Saved config '{}' to {}", name, path.display());
    Ok(())
}

/// Imports a config file from an external path into the configs directory.
pub fn import_config(source_path: &Path) -> Result<()> {
    let file_name = source_path.file_name()
        .context("Could not get file name from source path")?;

    let dest_path = get_configs_dir()?.join(file_name);

    fs::copy(source_path, &dest_path).with_context(|| {
        format!("Failed to copy config from {} to {}", source_path.display(), dest_path.display())
    })?;

    log::info!("Imported config to {}", dest_path.display());
    Ok(())
}

// --- NEW FUNCTION ---
/// Deletes a config file by name, with a safety check for the default config.
pub fn delete_config(name: &str) -> Result<()> {
    if name == "default" {
        anyhow::bail!("The default configuration cannot be deleted.");
    }

    let path = get_configs_dir()?.join(format!("{}.yaml", name));
    fs::remove_file(&path)
        .with_context(|| format!("Failed to delete config file at {}", path.display()))?;
    log::info!("Deleted config '{}'", name);
    Ok(())
}