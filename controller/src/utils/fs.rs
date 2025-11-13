// controller/src/utils/fs.rs

use std::path::PathBuf;
use winreg::enums::*;
use winreg::RegKey;

pub fn find_csgo_cfg_path() -> Option<PathBuf> {
    // 1. Find Steam installation path from the registry
    let hklm = RegKey::predef(HKEY_CURRENT_USER);
    let steam_key = hklm.open_subkey("Software\\Valve\\Steam").ok()?;
    let steam_path_str: String = steam_key.get_value("SteamPath").ok()?;
    let steam_path = PathBuf::from(steam_path_str);

    // 2. Navigate to the userdata directory
    let userdata_path = steam_path.join("userdata");
    if !userdata_path.is_dir() {
        return None;
    }

    // 3. Find the first user ID directory
    let user_id_dir = std::fs::read_dir(userdata_path)
        .ok()?
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
            && entry.file_name().to_string_lossy().chars().all(char::is_numeric)
        })?;

    // 4. Construct the final path to config.cfg
    let cfg_path = user_id_dir.path().join("730/local/cfg/config.cfg");
    
    if cfg_path.is_file() {
        Some(cfg_path)
    } else {
        None
    }
}