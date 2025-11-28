// controller/src/settings/config.rs

use directories::UserDirs;
use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
    fs::{
        self,
        File,
    },
    io::{
        BufReader,
        BufWriter,
    },
    path::PathBuf,
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
};

use anyhow::Context;
use imgui::Key;
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::with_prefix;
use utils_state::{
    State,
    StateCacheType,
};

use super::{
    esp::{
        Color,
        EspColor,
        EspConfig,
        EspPlayerSettings,
        EspBoxType,
        EspHeadDot,
        EspHealthBar,
        EspTracePosition,
        EspInfoStyle,
        EspTextStyle,
    },
    HotKey,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SniperCrosshairSettings {
    pub size: f32,
    pub thickness: f32,
    pub gap: f32,
    pub dot: bool,
    pub outline: bool,
    pub outline_thickness: f32,
    pub color: [u8; 4],
}

impl Default for SniperCrosshairSettings {
    fn default() -> Self {
        Self {
            size: 5.0,
            thickness: 1.0,
            gap: 1.0,
            dot: false,
            outline: true,
            outline_thickness: 1.0,
            color: [255, 255, 255, 255],
        }
    }
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub struct GrenadeTrajectorySettings {
    #[serde(default = "bool_true")]
    pub enabled: bool,
    #[serde(default = "default_color::<255, 255, 255, 255>")]
    pub line_color: Color,
    #[serde(default = "default_f32::<2, 1>")]
    pub line_thickness: f32,
}

impl Default for GrenadeTrajectorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            line_color: default_color::<255, 255, 255, 255>(),
            line_thickness: 2.0,
        }
    }
}

fn bool_true() -> bool { true }
fn default_f32<const N: usize, const D: usize>() -> f32 { N as f32 / D as f32 }
fn default_usize<const V: usize>() -> usize { V }
fn default_color<const R: u8, const G: u8, const B: u8, const A: u8>() -> Color { Color::from_u8([R, G, B, A]) }

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, PartialOrd)]
pub enum KeyToggleMode {
    AlwaysOn,
    Toggle,
    Trigger,
    TriggerInverted,
    Off,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum GrenadeType {
    Smoke,
    Molotov,
    Flashbang,
    Explosive,
}

impl GrenadeType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Smoke => "Smoke",
            Self::Molotov => "Molotov",
            Self::Flashbang => "Flashbang",
            Self::Explosive => "Explosive",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GrenadeSortOrder {
    Alphabetical,
    AlphabeticalReverse,
}

impl GrenadeSortOrder {
    pub fn default() -> Self { Self::Alphabetical }
    pub fn sort(&self, values: &mut Vec<&GrenadeSpotInfo>) {
        match self {
            Self::Alphabetical => { values.sort_unstable_by(|a, b| a.name.cmp(&b.name)); }
            Self::AlphabeticalReverse => { values.sort_unstable_by(|a, b| b.name.cmp(&b.name)); }
        }
    }
}

static GRENADE_SPOT_ID_INDEX: AtomicUsize = AtomicUsize::new(1);
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrenadeSpotInfo {
    #[serde(skip, default = "GrenadeSpotInfo::new_id")]
    pub id: usize,
    pub grenade_types: Vec<GrenadeType>,
    pub name: String,
    pub description: String,
    pub eye_position: [f32; 3],
    pub eye_direction: [f32; 2],
}

impl GrenadeSpotInfo {
    pub fn new_id() -> usize { GRENADE_SPOT_ID_INDEX.fetch_add(1, Ordering::Relaxed) }
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub struct GrenadeSettings {
    #[serde(default = "bool_true")]
    pub active: bool,
    #[serde(default = "GrenadeSortOrder::default")]
    pub ui_sort_order: GrenadeSortOrder,
    #[serde(default = "default_f32::<150, 1>")]
    pub circle_distance: f32,
    #[serde(default = "default_f32::<20, 1>")]
    pub circle_radius: f32,
    #[serde(default = "default_usize::<32>")]
    pub circle_segments: usize,
    #[serde(default = "default_f32::<1, 10>")]
    pub angle_threshold_yaw: f32,
    #[serde(default = "default_f32::<5, 10>")]
    pub angle_threshold_pitch: f32,
    #[serde(default = "default_color::<255, 255, 255, 255>")]
    pub color_position: Color,
    #[serde(default = "default_color::<0, 255, 0, 255>")]
    pub color_position_active: Color,
    #[serde(default = "default_color::<255, 0, 0, 255>")]
    pub color_angle: Color,
    #[serde(default = "default_color::<0, 255, 0, 255>")]
    pub color_angle_active: Color,
    #[serde(default)]
    pub map_spots: HashMap<String, Vec<GrenadeSpotInfo>>,
    #[serde(default = "bool_true")]
    pub grenade_background: bool,
}

impl Default for GrenadeSettings {
    fn default() -> Self {
        Self {
            active: bool_true(),
            ui_sort_order: GrenadeSortOrder::default(),
            circle_distance: default_f32::<150, 1>(),
            circle_radius: default_f32::<20, 1>(),
            circle_segments: default_usize::<32>(),
            angle_threshold_yaw: default_f32::<1, 10>(),
            angle_threshold_pitch: default_f32::<5, 10>(),
            color_position: default_color::<255, 255, 255, 255>(),
            color_position_active: default_color::<0, 255, 0, 255>(),
            color_angle: default_color::<255, 0, 0, 255>(),
            color_angle_active: default_color::<0, 255, 0, 255>(),
            map_spots: HashMap::new(),
            grenade_background: bool_true(),
        }
    }
}

with_prefix!(serde_prefix_grenade_helper "grenade_helper");

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    pub key_settings: HotKey,
    pub key_settings_ignore_insert_warning: bool,
    pub esp_mode: KeyToggleMode,
    pub esp_toggle: Option<HotKey>,
    pub esp_settings: BTreeMap<String, EspConfig>,
    pub esp_settings_enabled: BTreeMap<String, bool>,
    pub bomb_timer: bool,
    pub bomb_label: bool,
    pub spectators_list: bool,
    pub labh_watermark: bool,
    pub mouse_x_360: i32,
    pub trigger_bot_mode: KeyToggleMode,
    pub key_trigger_bot: Option<HotKey>,
    pub trigger_bot_team_check: bool,
    pub trigger_bot_delay_min: u32,
    pub trigger_bot_delay_max: u32,
    pub trigger_bot_shot_duration: u32,
    pub trigger_bot_check_target_after_delay: bool,
    pub aim_assist_recoil: bool,
    pub aim_assist_recoil_min_bullets: u32,
    pub hide_overlay_from_screen_capture: bool,
    pub render_debug_window: bool,
    pub metrics: bool,
    pub web_radar_url: Option<String>,
    pub web_radar_advanced_settings: bool,
    pub sniper_crosshair: bool,
    pub sniper_crosshair_settings: SniperCrosshairSettings,
    pub grenade_trajectory: GrenadeTrajectorySettings,
    #[serde(flatten, with = "serde_prefix_grenade_helper")]
    pub grenade_helper: GrenadeSettings,

    // Legit Aim Settings
    pub legit_aim_enabled: bool,
    pub legit_aim_fov: f32,
    pub legit_aim_smooth: f32,
    pub legit_aim_key: Option<HotKey>,
    pub legit_aim_bone: String,

    pub imgui: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        let white_color = EspColor::Static { value: Color::from_u8([255, 255, 255, 255]) };
        let green_color = EspColor::Static { value: Color::from_u8([0, 255, 0, 255]) };

        let enemy_settings = EspConfig::Player(EspPlayerSettings {
            box_type: EspBoxType::Box2D,
            box_color: white_color,
            box_width: 1.0,
            skeleton: true,
            skeleton_color: white_color,
            skeleton_width: 1.0,
            health_bar: EspHealthBar::Left,
            health_bar_width: 4.0,
            tracer_lines: EspTracePosition::None,
            tracer_lines_color: white_color,
            tracer_lines_width: 1.0,
            
            text_style: EspTextStyle::Shadow, // Default
            text_outline_enabled: false,
            text_outline_color: white_color,

            info_name: false,
            info_name_color: white_color,
            info_distance: false,
            info_distance_color: white_color,
            near_players: false,
            near_players_distance: 20.0,
            
            info_weapon: false,
            info_weapon_style: EspInfoStyle::Text,
            info_weapon_color: white_color,
            
            info_ammo: false,
            info_ammo_color: white_color,
            info_hp_text: false,
            info_hp_text_color: green_color,
            info_flag_kit: false,
            info_flag_scoped: false,
            info_flag_flashed: false,
            info_flag_bomb: false,
            info_flag_kit_color: white_color,
            info_flag_scoped_color: white_color,
            info_flag_flashed_color: white_color,
            info_flag_bomb_color: white_color,
            info_grenades: false,
            info_grenades_color: white_color,
            
            // --- OFFSCREEN ARROWS (ADDED) ---
            offscreen_arrows: false,
            offscreen_arrows_color: white_color,
            offscreen_arrows_radius: 300.0,
            offscreen_arrows_size: 15.0,
            // --------------------------------

            head_dot: EspHeadDot::NotFilled,
            head_dot_color: white_color,
            head_dot_thickness: 1.0,
            head_dot_base_radius: 12.0, 
            head_dot_z: 1.0,
            chams: false,
            chams_color: white_color,
        });

        let mut friendly_settings = enemy_settings;
        if let EspConfig::Player(ref mut p) = friendly_settings {
            p.skeleton = false; 
        }

        Self {
            key_settings: Key::Insert.into(),
            key_settings_ignore_insert_warning: false,
            esp_mode: KeyToggleMode::AlwaysOn,
            esp_toggle: None,
            esp_settings: BTreeMap::from([
                ("player.enemy".to_string(), enemy_settings),
                ("player.friendly".to_string(), friendly_settings),
            ]),
            esp_settings_enabled: BTreeMap::from([
                ("player.enemy".to_string(), true),
                ("player.friendly".to_string(), true),
            ]),
            bomb_timer: true,
            bomb_label: true,
            spectators_list: false,
            labh_watermark: true,
            mouse_x_360: 16364,
            trigger_bot_mode: KeyToggleMode::Trigger,
            key_trigger_bot: Some(Key::MouseMiddle.into()),
            trigger_bot_team_check: true,
            trigger_bot_delay_min: 10,
            trigger_bot_delay_max: 20,
            trigger_bot_shot_duration: 400,
            trigger_bot_check_target_after_delay: false,
            aim_assist_recoil: false,
            aim_assist_recoil_min_bullets: 1,
            hide_overlay_from_screen_capture: false,
            render_debug_window: false,
            metrics: true,
            web_radar_url: None,
            web_radar_advanced_settings: false,
            sniper_crosshair: true,
            sniper_crosshair_settings: Default::default(),
            grenade_trajectory: GrenadeTrajectorySettings::default(),
            grenade_helper: GrenadeSettings::default(),

            legit_aim_enabled: false,
            legit_aim_fov: 100.0, // Pixel radius
            legit_aim_smooth: 5.0,
            legit_aim_key: Some(Key::MouseX1.into()), // Default to Mouse Button 4
            legit_aim_bone: "head_0".to_string(),

            imgui: None,
        }
    }
}

impl State for AppSettings {
    type Parameter = ();
    fn cache_type() -> StateCacheType { StateCacheType::Persistent }
}

pub fn get_managed_configs_dir() -> anyhow::Result<PathBuf> {
    let user_dirs = UserDirs::new().context("failed to get user directories")?;
    let documents_dir = user_dirs.document_dir().context("failed to find documents directory")?;
    let managed_configs_dir = documents_dir.join("LABHConfig").join("configs");

    fs::create_dir_all(&managed_configs_dir).with_context(|| format!("Failed to create managed configs directory at {}", managed_configs_dir.display()))?;
    
    Ok(managed_configs_dir)
}


pub fn get_settings_path() -> anyhow::Result<PathBuf> {
    let config_dir = get_managed_configs_dir()?;
    Ok(config_dir.join("default.yaml"))
}

pub fn load_app_settings() -> anyhow::Result<AppSettings> {
    let config_path = get_settings_path()?;
    if !config_path.is_file() {
        log::info!("App config file {} does not exist. Creating default.", config_path.to_string_lossy());
        let config = AppSettings::default();
        save_app_settings(&config)?;
        return Ok(config);
    }
    
    let file = File::open(&config_path).with_context(|| format!("failed to open app config at {}", config_path.to_string_lossy()))?;
    let mut reader = BufReader::new(file);
    let mut config: AppSettings = serde_yaml::from_reader(&mut reader).context("failed to parse app config")?;
    
    if config.imgui.is_none() {
        log::info!("Existing config is missing imgui settings. Injecting defaults.");
        config.imgui = AppSettings::default().imgui;
    }

    log::info!("Loaded app config from {}", config_path.to_string_lossy());
    Ok(config)
}

pub fn save_app_settings(settings: &AppSettings) -> anyhow::Result<()> {
    let config_path = get_settings_path()?;
    let config = File::options().create(true).truncate(true).write(true).open(&config_path).with_context(|| format!("failed to open app config at {}", config_path.to_string_lossy()))?;
    let mut config = BufWriter::new(config);
    serde_yaml::to_writer(&mut config, settings).context("failed to serialize config")?;
    log::debug!("Saved app config.");
    Ok(())
}