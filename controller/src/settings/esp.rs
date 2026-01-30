// controller/src/settings/esp.rs

use cs2::{
    WeaponId,
    WEAPON_FLAG_TYPE_GRENADE,
    WEAPON_FLAG_TYPE_MACHINE_GUN,
    WEAPON_FLAG_TYPE_PISTOL,
    WEAPON_FLAG_TYPE_RIFLE,
    WEAPON_FLAG_TYPE_SHOTGUN,
    WEAPON_FLAG_TYPE_SMG,
    WEAPON_FLAG_TYPE_SNIPER_RIFLE,
};
use obfstr::obfstr;
use serde::{
    Deserialize,
    Serialize,
};
use std::collections::HashMap;
use imgui::{DrawListMut, TextureId, Ui};
use crate::AppResources;

#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, PartialOrd)]
pub struct Color(u32);
impl Color {
    pub fn as_u8(&self) -> [u8; 4] { self.0.to_le_bytes() }
    pub fn as_f32(&self) -> [f32; 4] { self.as_u8().map(|channel| (channel as f32) / (u8::MAX as f32)) }
    pub const fn from_u8(value: [u8; 4]) -> Self { Self(u32::from_le_bytes(value)) }
    pub const fn from_u32(value: u32) -> Self { Self(value) }
    pub const fn from_f32(value: [f32; 4]) -> Self {
        Self::from_u8([
            (value[0] * 255.0) as u8,
            (value[1] * 255.0) as u8,
            (value[2] * 255.0) as u8,
            (value[3] * 255.0) as u8,
        ])
    }
    pub fn set_alpha_u8(&mut self, alpha: u8) { let mut value = self.as_u8(); value[3] = alpha; *self = Self::from_u8(value); }
    pub fn set_alpha_f32(&mut self, alpha: f32) { let mut value = self.as_u8(); value[3] = (alpha * 255.0) as u8; *self = Self::from_u8(value); }
}
impl From<[u8; 4]> for Color { fn from(value: [u8; 4]) -> Self { Self::from_u8(value) } }
impl From<[f32; 4]> for Color { fn from(value: [f32; 4]) -> Self { Self::from_f32(value) } }

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd)]
#[serde(tag = "type", content = "options")]
pub enum EspColor {
    HealthBasedRainbow { alpha: f32 },
    HealthBased { max: Color, mid: Color, min: Color },
    Static { value: Color },
    DistanceBased { near: Color, mid: Color, far: Color },
    #[serde(alias = "Gradient")]
    GradientPulse { start: Color, end: Color, speed: f32 },
    GradientVertical { top: Color, bottom: Color },
}

impl Default for EspColor { fn default() -> Self { Self::Static { value: Color::from_f32([1.0, 1.0, 1.0, 1.0]), } } }

impl EspColor {
    pub const fn from_rgba(r: f32, g: f32, b: f32, a: f32) -> Self { Self::Static { value: Color::from_f32([r, g, b, a]), } }

    fn interpolate_color(start: [f32; 4], end: [f32; 4], t: f32) -> [f32; 4] {
        [
            start[0] + (end[0] - start[0]) * t,
            start[1] + (end[1] - start[1]) * t,
            start[2] + (end[2] - start[2]) * t,
            start[3] + (end[3] - start[3]) * t,
        ]
    }

    pub fn calculate_color(&self, health: f32, distance: f32, time: f32, vertical_t: f32) -> [f32; 4] {
        match self {
            Self::Static { value } => value.as_f32(),
            Self::HealthBased { max, mid, min } => {
                let max_rgb = max.as_f32(); let mid_rgb = mid.as_f32(); let min_rgb = min.as_f32();
                if health > 0.5 { Self::interpolate_color(mid_rgb, max_rgb, (health - 0.5) * 2.0) } 
                else { Self::interpolate_color(min_rgb, mid_rgb, health * 2.0) }
            }
            Self::HealthBasedRainbow { alpha } => {
                let sin_value = |offset: f32| { (2.0 * std::f32::consts::PI * health * 0.75 + offset).sin() * 0.5 + 1.0 };
                [sin_value(0.0), sin_value(2.0 * std::f32::consts::PI / 3.0), sin_value(4.0 * std::f32::consts::PI / 3.0), *alpha]
            }
            Self::DistanceBased { near, mid, far } => {
                let t = ((distance) / 50.0).clamp(0.0, 1.0);
                let (near, mid, far) = (near.as_f32(), mid.as_f32(), far.as_f32());
                if t < 0.5 { Self::interpolate_color(near, mid, t * 2.0) } else { Self::interpolate_color(mid, far, (t - 0.5) * 2.0) }
            }
            Self::GradientPulse { start, end, speed } => {
                let t = (time * speed).sin() * 0.5 + 0.5;
                Self::interpolate_color(start.as_f32(), end.as_f32(), t)
            }
            Self::GradientVertical { top, bottom } => {
                Self::interpolate_color(bottom.as_f32(), top.as_f32(), vertical_t.clamp(0.0, 1.0))
            }
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd)]
pub enum EspColorType { Static, HealthBased, HealthBasedRainbow, DistanceBased, GradientPulse, GradientVertical }
impl EspColorType {
    pub fn from_esp_color(color: &EspColor) -> Self {
        match color {
            EspColor::Static { .. } => Self::Static,
            EspColor::HealthBased { .. } => Self::HealthBased,
            EspColor::HealthBasedRainbow { .. } => Self::HealthBasedRainbow,
            EspColor::DistanceBased { .. } => Self::DistanceBased,
            EspColor::GradientPulse { .. } => Self::GradientPulse,
            EspColor::GradientVertical { .. } => Self::GradientVertical,
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd)] pub enum EspHealthBar { None, Top, Bottom, Left, Right }
#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd)] pub enum EspBoxType { None, Box2D, Box3D }
#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd)] pub enum EspTracePosition { None, TopLeft, TopCenter, TopRight, Center, BottomLeft, BottomCenter, BottomRight }
#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd)] pub enum EspHeadDot { None, Filled, NotFilled }
#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd, Debug)] pub enum EspInfoStyle { Text, Icon }
#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd, Debug)] pub enum EspTextStyle { Shadow, Outline, Neon }

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd)]
#[serde(default)]
pub struct EspPlayerSettings {
    pub box_type: EspBoxType,
    pub box_color: EspColor,
    pub box_width: f32,
    pub skeleton: bool,
    pub skeleton_color: EspColor,
    pub skeleton_width: f32,
    pub health_bar: EspHealthBar,
    pub health_bar_width: f32,
    pub tracer_lines: EspTracePosition,
    pub tracer_lines_color: EspColor,
    pub tracer_lines_width: f32,
    pub text_style: EspTextStyle,
    pub text_outline_enabled: bool,
    pub text_outline_color: EspColor,
    pub info_name: bool,
    pub info_name_color: EspColor,
    pub info_distance: bool,
    pub info_distance_color: EspColor,
    pub near_players: bool,
    pub near_players_distance: f32,
    pub info_weapon: bool,
    pub info_weapon_style: EspInfoStyle,
    pub info_weapon_color: EspColor,
    pub info_ammo: bool,
    pub info_ammo_color: EspColor,
    pub info_hp_text: bool,
    pub info_hp_text_color: EspColor,
    pub info_flag_kit: bool,
    pub info_flag_scoped: bool,
    pub info_flag_flashed: bool,
    pub info_flag_bomb: bool,
    pub info_flag_kit_color: EspColor,
    pub info_flag_scoped_color: EspColor,
    pub info_flag_flashed_color: EspColor,
    pub info_flag_bomb_color: EspColor,
    pub info_grenades: bool,
    pub info_grenades_color: EspColor,
    // --- OFFSCREEN ARROWS ---
    pub offscreen_arrows: bool,
    pub offscreen_arrows_color: EspColor,
    pub offscreen_arrows_radius: f32,
    pub offscreen_arrows_size: f32,
    // ------------------------
    pub head_dot: EspHeadDot,
    pub head_dot_color: EspColor,
    pub head_dot_thickness: f32,
    pub head_dot_base_radius: f32,
    pub head_dot_z: f32,
    pub chams: bool,
    pub chams_color: EspColor,
}

const ESP_COLOR_FRIENDLY: EspColor = EspColor::from_rgba(0.0, 1.0, 0.0, 0.75);
const ESP_COLOR_ENEMY: EspColor = EspColor::from_rgba(1.0, 0.0, 0.0, 0.75);
impl EspPlayerSettings {
    pub fn new(target: &EspSelector) -> Self {
        let color = match target {
            EspSelector::PlayerTeam { enemy } => { if *enemy { ESP_COLOR_ENEMY } else { ESP_COLOR_FRIENDLY } }
            EspSelector::PlayerTeamVisibility { enemy, .. } => { if *enemy { ESP_COLOR_ENEMY } else { ESP_COLOR_FRIENDLY } }
            _ => EspColor::from_rgba(1.0, 1.0, 1.0, 0.75),
        };
        Self {
            box_type: EspBoxType::None, box_color: color, box_width: 1.0,
            skeleton: true, skeleton_color: color, skeleton_width: 1.0,
            health_bar: EspHealthBar::None, health_bar_width: 4.0,
            tracer_lines: EspTracePosition::None, tracer_lines_color: color, tracer_lines_width: 1.0,
            text_style: EspTextStyle::Shadow,
            text_outline_enabled: false, text_outline_color: color,
            info_name: false, info_name_color: color,
            info_distance: false, info_distance_color: color,
            near_players: false, near_players_distance: 20.0,
            info_weapon: false, info_weapon_style: EspInfoStyle::Text, info_weapon_color: color,
            info_ammo: false, info_ammo_color: color,
            info_hp_text: false, info_hp_text_color: color,
            info_flag_kit: false, info_flag_scoped: false, info_flag_flashed: false, info_flag_bomb: false,
            info_flag_kit_color: color, info_flag_scoped_color: color, info_flag_flashed_color: color, info_flag_bomb_color: color,
            info_grenades: false, info_grenades_color: color,
            // --- OFFSCREEN ARROWS ---
            offscreen_arrows: false, 
            offscreen_arrows_color: color,
            offscreen_arrows_radius: 300.0,
            offscreen_arrows_size: 15.0,
            // ------------------------
            head_dot: EspHeadDot::None, head_dot_color: color, head_dot_thickness: 1.0, head_dot_base_radius: 4.0, head_dot_z: 1.0,
            chams: false, chams_color: color,
        }
    }
}

impl Default for EspPlayerSettings {
    fn default() -> Self {
        let neutral_color = EspColor::from_rgba(1.0, 1.0, 1.0, 0.75);
        Self {
            box_type: EspBoxType::Box2D, box_color: neutral_color, box_width: 1.0,
            skeleton: true, skeleton_color: neutral_color, skeleton_width: 1.0,
            health_bar: EspHealthBar::Left, health_bar_width: 4.0,
            tracer_lines: EspTracePosition::None, tracer_lines_color: neutral_color, tracer_lines_width: 1.0,
            text_style: EspTextStyle::Shadow,
            text_outline_enabled: false, text_outline_color: neutral_color,
            info_name: true, info_name_color: neutral_color,
            info_distance: true, info_distance_color: neutral_color,
            near_players: false, near_players_distance: 20.0,
            info_weapon: true, info_weapon_style: EspInfoStyle::Text, info_weapon_color: neutral_color,
            info_ammo: false, info_ammo_color: neutral_color,
            info_hp_text: false, info_hp_text_color: neutral_color,
            info_flag_kit: true, info_flag_scoped: true, info_flag_flashed: true, info_flag_bomb: true,
            info_flag_kit_color: neutral_color, info_flag_scoped_color: neutral_color, info_flag_flashed_color: neutral_color, info_flag_bomb_color: neutral_color,
            info_grenades: false, info_grenades_color: neutral_color,
            // --- OFFSCREEN ARROWS ---
            offscreen_arrows: false,
            offscreen_arrows_color: neutral_color,
            offscreen_arrows_radius: 300.0,
            offscreen_arrows_size: 15.0,
            // ------------------------
            head_dot: EspHeadDot::NotFilled, head_dot_color: neutral_color, head_dot_thickness: 1.0, head_dot_base_radius: 4.0, head_dot_z: 1.0,
            chams: false, chams_color: neutral_color,
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd)] #[serde(default)] pub struct EspChickenSettings { pub box_type: EspBoxType, pub box_color: EspColor, pub skeleton: bool, pub skeleton_color: EspColor, pub info_owner: bool, pub info_owner_color: EspColor, }
impl Default for EspChickenSettings { fn default() -> Self { Self { box_type: EspBoxType::None, box_color: EspColor::default(), skeleton: false, skeleton_color: EspColor::default(), info_owner: false, info_owner_color: EspColor::default(), } } }
#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd)] #[serde(default)] pub struct EspWeaponSettings { pub draw_box: bool, pub draw_box_color: EspColor, pub info_name: bool, pub info_name_color: EspColor, }
impl Default for EspWeaponSettings { fn default() -> Self { Self { draw_box: false, draw_box_color: EspColor::default(), info_name: false, info_name_color: EspColor::default(), } } }
#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, PartialOrd)] #[serde(tag = "type")] pub enum EspConfig { Player(EspPlayerSettings), Chicken(EspChickenSettings), Weapon(EspWeaponSettings), }

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum EspWeaponType { Pistol, Shotgun, SMG, Rifle, SniperRifle, MachineGun, Grenade, }

impl EspWeaponType {
    pub fn display_name(&self) -> String {
        match self {
            Self::Pistol => "Pistol", Self::Shotgun => "Shotgun", Self::SMG => "SMG",
            Self::Rifle => "Rifle", Self::SniperRifle => "Sniper Rifle",
            Self::MachineGun => "Machine Gun", Self::Grenade => "Grenade",
        }.to_string()
    }
    pub fn config_key(&self) -> &'static str {
        match self {
            Self::Pistol => "pistol", Self::Shotgun => "shotgun", Self::SMG => "smg",
            Self::Rifle => "rifle", Self::SniperRifle => "sniper-rifle",
            Self::MachineGun => "machine-gun", Self::Grenade => "grenade",
        }
    }
    pub fn weapons(&self) -> Vec<WeaponId> {
        let flag = match self {
            Self::Pistol => WEAPON_FLAG_TYPE_PISTOL, Self::Shotgun => WEAPON_FLAG_TYPE_SHOTGUN,
            Self::SMG => WEAPON_FLAG_TYPE_SMG, Self::Rifle => WEAPON_FLAG_TYPE_RIFLE,
            Self::SniperRifle => WEAPON_FLAG_TYPE_SNIPER_RIFLE,
            Self::MachineGun => WEAPON_FLAG_TYPE_MACHINE_GUN, Self::Grenade => WEAPON_FLAG_TYPE_GRENADE,
        };
        WeaponId::all_weapons().into_iter().filter(|weapon| (weapon.flags() & flag) > 0).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum EspSelector {
    None, Player, PlayerTeam { enemy: bool }, PlayerTeamVisibility { enemy: bool, visible: bool },
    Chicken, Weapon, WeaponGroup { group: EspWeaponType }, WeaponSingle { group: EspWeaponType, target: WeaponId },
}

impl EspSelector {
    pub fn config_key(&self) -> String {
        match self {
            EspSelector::None => "invalid".to_string(),
            EspSelector::Player => "player".to_string(),
            EspSelector::PlayerTeam { enemy } => { format!("player.{}", if *enemy { "enemy" } else { "friendly" },) }
            EspSelector::PlayerTeamVisibility { enemy, visible } => format!( "player.{}.{}", if *enemy { "enemy" } else { "friendly" }, if *visible { "visible" } else { "occluded" } ),
            EspSelector::Chicken => "chicken".to_string(),
            EspSelector::Weapon => format!("weapon"),
            EspSelector::WeaponGroup { group } => format!("weapon.{}", group.config_key()),
            EspSelector::WeaponSingle { group, target } => { format!("weapon.{}.{}", group.config_key(), target.name()) }
        }
    }

    pub fn config_display(&self) -> String {
        match self {
            EspSelector::None => "None".to_string(),
            EspSelector::Player => "Player".to_string(),
            EspSelector::PlayerTeam { enemy } => { if *enemy { "Enemy".to_string() } else { "Friendly".to_string() } }
            EspSelector::PlayerTeamVisibility { visible, .. } => { if *visible { "Visible".to_string() } else { "Occluded".to_string() } }
            EspSelector::Chicken => "Chicken".to_string(),
            EspSelector::Weapon => "Weapons".to_string(),
            EspSelector::WeaponGroup { group } => group.display_name(),
            EspSelector::WeaponSingle { target, .. } => target.display_name().to_string(),
        }
    }

    pub fn config_title(&self) -> String {
        match self {
            EspSelector::None => obfstr!("ESP Configuration").to_string(),
            EspSelector::Player => obfstr!("Enabled ESP for all players").to_string(),
            EspSelector::PlayerTeam { enemy } => format!( "{} {} players", obfstr!("Enabled ESP for"), if *enemy { "enemy" } else { "friendly" } ),
            EspSelector::PlayerTeamVisibility { enemy, visible } => format!( "{} {} {} players", obfstr!("Enabled ESP for"), if *visible { "visible" } else { "occluded" }, if *enemy { "enemy" } else { "friendly" } ),
            EspSelector::Chicken => obfstr!("Enabled ESP for chickens").to_string(),
            EspSelector::Weapon => obfstr!("Enabled ESP for all weapons").to_string(),
            EspSelector::WeaponGroup { group } => { format!( "{} {}", obfstr!("Enabled ESP for"), group.display_name().to_lowercase() ) }
            EspSelector::WeaponSingle { target, .. } => { format!( "{} {}", obfstr!("Enabled ESP for weapon"), target.display_name() ) }
        }
    }

    pub fn parent(&self) -> Option<Self> {
        match self {
            Self::None => None,
            Self::Player => None,
            Self::PlayerTeam { .. } => Some(Self::Player),
            Self::PlayerTeamVisibility { enemy, .. } => Some(Self::PlayerTeam { enemy: *enemy }),
            Self::Chicken => None,
            Self::Weapon => None,
            Self::WeaponGroup { .. } => Some(Self::Weapon),
            Self::WeaponSingle { group, .. } => Some(Self::WeaponGroup { group: *group }),
        }
    }

    pub fn children(&self) -> Vec<Self> {
        match self {
            EspSelector::None => vec![],
            EspSelector::Player => vec![ EspSelector::PlayerTeam { enemy: false }, EspSelector::PlayerTeam { enemy: true }, ],
            EspSelector::PlayerTeam { .. } => vec![],
            EspSelector::PlayerTeamVisibility { .. } => vec![],
            EspSelector::Chicken => vec![],
            EspSelector::Weapon => vec![
                EspSelector::WeaponGroup { group: EspWeaponType::Pistol },
                EspSelector::WeaponGroup { group: EspWeaponType::SMG },
                EspSelector::WeaponGroup { group: EspWeaponType::Shotgun },
                EspSelector::WeaponGroup { group: EspWeaponType::Rifle },
                EspSelector::WeaponGroup { group: EspWeaponType::SniperRifle },
                EspSelector::WeaponGroup { group: EspWeaponType::Grenade },
            ],
            EspSelector::WeaponGroup { group } => group.weapons().into_iter().map(|weapon| EspSelector::WeaponSingle { group: *group, target: weapon }).collect(),
            EspSelector::WeaponSingle { .. } => vec![],
        }
    }
}

pub struct EspRenderInfo<'a> {
    pub bones: &'a HashMap<String, [f32; 2]>,
    pub model_bounds: Option<([f32; 2], [f32; 2])>,
    pub skeleton_lines: Option<&'a Vec<([f32; 2], [f32; 2])>>,
    pub health: f32,
    pub distance: f32,
    pub name: &'a str,
    pub weapon_name: &'a str,
    pub weapon_icon_name: Option<&'a str>,
    pub team_indicator: &'a str,
    pub is_scoped: bool,
    pub is_flashed: bool,
    pub has_kit: bool,
    pub has_bomb: bool,
}

/// Draws an image with a solid outline effect that matches the primary color.
fn draw_image_with_thickness(
    draw_list: &DrawListMut,
    texture_id: TextureId,
    pos: [f32; 2],
    size: [f32; 2],
    thickness: f32,
    color: [f32; 4],
) {
    let clamped_thickness = thickness.clamp(0.0, 5.0);

    if clamped_thickness <= 1.0 {
        draw_list.add_image(texture_id, pos, [pos[0] + size[0], pos[1] + size[1]])
            .col(color)
            .build();
        return;
    }

    // Create a solid black color with the same alpha as the main color for the outline.
    let outline_color = [0.0, 0.0, 0.0, color[3]];

    // Define 8-way offsets for a smoother outline.
    let offsets = [
        [-clamped_thickness, -clamped_thickness], [ 0.0, -clamped_thickness], [clamped_thickness, -clamped_thickness],
        [-clamped_thickness, 0.0],                                            [clamped_thickness, 0.0],
        [-clamped_thickness, clamped_thickness],  [ 0.0, clamped_thickness],  [clamped_thickness, clamped_thickness],
    ];

    let end_pos = [pos[0] + size[0], pos[1] + size[1]];

    // Draw the eight offset background copies tinted with the solid outline color.
    for offset in offsets {
        let offset_pos = [pos[0] + offset[0], pos[1] + offset[1]];
        let offset_end_pos = [end_pos[0] + offset[0], end_pos[1] + offset[1]];
        draw_list.add_image(texture_id, offset_pos, offset_end_pos)
            .col(outline_color)
            .build();
    }

    // Draw the main, centered image on top, tinted with the bright ESP color.
    draw_list.add_image(texture_id, pos, end_pos)
        .col(color)
        .build();
}

pub fn draw_player_esp(
    draw_list: &DrawListMut,
    ui: &Ui,
    settings: &EspPlayerSettings,
    info: &EspRenderInfo,
    _area_pos: [f32; 2],
    _area_size: [f32; 2],
    alpha: f32,
    resources: &AppResources,
    time: f32, 
) {
    if info.bones.is_empty() { return; }

    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for pos in info.bones.values() {
        min_x = min_x.min(pos[0]);
        min_y = min_y.min(pos[1]);
        max_x = max_x.max(pos[0]);
        max_y = max_y.max(pos[1]);
    }

    if min_x == f32::MAX { return; }

    // Use model bounds if available, otherwise fallback to bones
    let (box_pos, box_size) = if let Some((min, max)) = info.model_bounds {
        // model bounds are exact screen pixels
        // Add small padding like in-game usually does?
        // Game code usually uses Hull Min/Max projected.
        // Let's assume passed bounds are tight and add padding.
        // Actually, player_2d_box in game uses Hull.
        // Let's add standard padding.
        let padding = 1.0; 
        let width = max[0] - min[0];
        let height = max[1] - min[1];
        ([min[0] - padding, min[1] - padding], [width + padding * 2.0, height + padding * 2.0])
    } else {
        let box_padding = 23.0;
        ([min_x - box_padding, min_y - box_padding], [(max_x - min_x) + box_padding * 2.0, (max_y - min_y) + box_padding * 2.0])
    };
    
    let get_t = |y_pos: f32| -> f32 {
        let box_h = box_size[1];
        if box_h > 0.1 { (y_pos - box_pos[1]) / box_h } else { 0.0 }
    };

    if settings.chams {
        if let Some((texture_id, _)) = resources.esp_preview_skeleton_texture_id {
            let t_skeleton = 0.5;
            let mut color = settings.chams_color.calculate_color(info.health, info.distance, time, t_skeleton);
            color[3] *= alpha;

            let (mut body_min_x, mut body_min_y, mut body_max_x, mut body_max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for (name, pos) in info.bones.iter() {
                if name != "head" {
                    body_min_x = body_min_x.min(pos[0]);
                    body_min_y = body_min_y.min(pos[1]);
                    body_max_x = body_max_x.max(pos[0]);
                    body_max_y = body_max_y.max(pos[1]);
                }
            }
            
            if body_min_x < f32::MAX {
                let body_pos = [body_min_x, body_min_y];
                let body_size = [body_max_x - body_min_x, body_max_y - body_min_y];
                draw_image_with_thickness(draw_list, texture_id, body_pos, body_size, 5.0, color);
            }
        }
    }

    if settings.skeleton {
        let t_skeleton = 0.5;
        let mut color = settings.skeleton_color.calculate_color(info.health, info.distance, time, t_skeleton);
        color[3] *= alpha;

        if let Some(lines) = info.skeleton_lines {
             for (start, end) in lines {
                 draw_list.add_line(*start, *end, color).thickness(settings.skeleton_width).build();
             }
        } else {
            // Bone connections (parent -> child)
            const BONE_PAIRS: &[(&str, &str)] = &[
                ("head_0", "neck_0"),
                ("neck_0", "spine_3"),
                ("spine_3", "spine_2"),
                ("spine_2", "spine_1"),
                ("spine_1", "pelvis"),
                ("spine_3", "arm_upper_L"), ("arm_upper_L", "arm_lower_L"), ("arm_lower_L", "hand_L"),
                ("spine_3", "arm_upper_R"), ("arm_upper_R", "arm_lower_R"), ("arm_lower_R", "hand_R"),
                ("pelvis", "leg_upper_L"), ("leg_upper_L", "leg_lower_L"), ("leg_lower_L", "ankle_L"),
                ("pelvis", "leg_upper_R"), ("leg_upper_R", "leg_lower_R"), ("leg_lower_R", "ankle_R"),
            ];

            for (p, c) in BONE_PAIRS {
                if let (Some(p_pos), Some(c_pos)) = (info.bones.get(*p), info.bones.get(*c)) {
                    let mid_y = (p_pos[1] + c_pos[1]) / 2.0;
                    let t = get_t(mid_y);
                    let mut color = settings.skeleton_color.calculate_color(info.health, info.distance, time, 1.0 - t);
                    color[3] *= alpha;
                    draw_list.add_line(*p_pos, *c_pos, color).thickness(settings.skeleton_width).build();
                }
            }
        }
    }
    
    // Try both "head" (old) and "head_0" (new model)
    if let Some(head_pos) = info.bones.get("head").or_else(|| info.bones.get("head_0")) {
        if settings.head_dot != EspHeadDot::None {
            let t_head = get_t(head_pos[1]);
            let mut color = settings.head_dot_color.calculate_color(info.health, info.distance, time, t_head);
            color[3] *= alpha;

            let radius = (box_size[1] / 36.0) * settings.head_dot_base_radius * 0.95;
            let circle = draw_list.add_circle([head_pos[0], head_pos[1]], radius, color);
            
            match settings.head_dot {
                EspHeadDot::Filled => { circle.filled(true).build(); }
                EspHeadDot::NotFilled => { circle.filled(false).thickness(settings.head_dot_thickness).build(); }
                EspHeadDot::None => {}
            }
        }
    }

    if settings.box_type == EspBoxType::Box2D {
         if let EspColor::GradientVertical { top, bottom } = settings.box_color {
             let mut c_top = top.as_f32(); let mut c_bot = bottom.as_f32();
             c_top[3] *= alpha;
             c_bot[3] *= alpha;
             
             // Top Line
             draw_list.add_rect([box_pos[0], box_pos[1]], [box_pos[0] + box_size[0], box_pos[1] + settings.box_width], c_top).filled(true).build();
             // Bottom Line
             draw_list.add_rect([box_pos[0], box_pos[1] + box_size[1] - settings.box_width], [box_pos[0] + box_size[0], box_pos[1] + box_size[1]], c_bot).filled(true).build();
             // Left Line
             draw_list.add_rect_filled_multicolor([box_pos[0], box_pos[1]], [box_pos[0] + settings.box_width, box_pos[1] + box_size[1]], c_top, c_top, c_bot, c_bot);
             // Right Line
             draw_list.add_rect_filled_multicolor([box_pos[0] + box_size[0] - settings.box_width, box_pos[1]], [box_pos[0] + box_size[0], box_pos[1] + box_size[1]], c_top, c_top, c_bot, c_bot);

         } else {
             let mut color = settings.box_color.calculate_color(info.health, info.distance, time, 0.0);
             color[3] *= alpha;
             // Use standard rect instead of image
             draw_list.add_rect(box_pos, [box_pos[0] + box_size[0], box_pos[1] + box_size[1]], color)
                .thickness(settings.box_width)
                .build();
         }
    }
    
    if settings.health_bar != EspHealthBar::None {
        let hp_percent = info.health.clamp(0.0, 1.0);
        let bar_width = settings.health_bar_width;
        let gap = 2.0;
        
        let (rect_min, rect_max) = match settings.health_bar {
            EspHealthBar::Left => {
                let x = box_pos[0] - bar_width - gap;
                ([x, box_pos[1]], [x + bar_width, box_pos[1] + box_size[1]])
            },
            EspHealthBar::Right => {
                let x = box_pos[0] + box_size[0] + gap;
                ([x, box_pos[1]], [x + bar_width, box_pos[1] + box_size[1]])
            },
            EspHealthBar::Top => {
                let y = box_pos[1] - bar_width - gap;
                ([box_pos[0], y], [box_pos[0] + box_size[0], y + bar_width])
            },
            EspHealthBar::Bottom => {
                let y = box_pos[1] + box_size[1] + gap;
                ([box_pos[0], y], [box_pos[0] + box_size[0], y + bar_width])
            },
            EspHealthBar::None => unreachable!(),
        };

        // Draw background
        draw_list.add_rect(rect_min, rect_max, [0.0, 0.0, 0.0, 0.5 * alpha]).filled(true).build();
        
        // Draw fill
        let mut color = settings.info_hp_text_color.calculate_color(info.health, info.distance, time, 0.5);
        color[3] *= alpha;

        let (fill_min, fill_max) = match settings.health_bar {
            EspHealthBar::Left | EspHealthBar::Right => {
                let h = rect_max[1] - rect_min[1];
                let fill_h = h * hp_percent;
                ([rect_min[0], rect_max[1] - fill_h], rect_max)
            },
            EspHealthBar::Top | EspHealthBar::Bottom => {
                let w = rect_max[0] - rect_min[0];
                let fill_w = w * hp_percent;
                (rect_min, [rect_min[0] + fill_w, rect_max[1]])
            }
            _ => (rect_min, rect_max),
        };
        
        draw_list.add_rect(fill_min, fill_max, color).filled(true).build();
        // Outline
        draw_list.add_rect(rect_min, rect_max, [0.0, 0.0, 0.0, 1.0 * alpha]).thickness(1.0).build();
    }

    let mut cursor_y = box_pos[1] + box_size[1] + 4.0;
    let box_center_x = box_pos[0] + box_size[0] / 2.0;

    ui.set_window_font_scale(1.5);
    if settings.info_name {
        let mut color = settings.info_name_color.calculate_color(info.health, info.distance, time, 0.0);
        color[3] *= alpha;
        let width = ui.calc_text_size(info.name)[0];
        let pos = [box_center_x - width / 2.0, cursor_y];
        draw_list.add_text(pos, color, info.name);
        cursor_y += 21.0;
    }

    if settings.info_ammo {
        let mut color = settings.info_ammo_color.calculate_color(info.health, info.distance, time, 0.0);
        color[3] *= alpha;
        let text = "30/90";
        let width = ui.calc_text_size(text)[0];
        let pos = [box_center_x - width / 2.0, cursor_y];
        draw_list.add_text(pos, color, text);
        cursor_y += 21.0;
    }

    if settings.info_distance {
        let mut color = settings.info_distance_color.calculate_color(info.health, info.distance, time, 0.0);
        color[3] *= alpha;
        let text = format!("{:.0}m", info.distance);
        let width = ui.calc_text_size(&text)[0];
        let pos = [box_center_x - width / 2.0, cursor_y];
        draw_list.add_text(pos, color, text);
        cursor_y += 21.0;
    }

    if settings.info_weapon {
        let mut color = settings.info_weapon_color.calculate_color(info.health, info.distance, time, 0.0);
        color[3] *= alpha;
        
        // Try to draw icon if available
        let icon_key = info.weapon_icon_name.unwrap_or(info.weapon_name);
        if let Some(tex_id) = resources.weapon_icons.get(icon_key) {
             // Standard size roughly 20px height
             let h = 38.25;
             let w = h * 2.5; // Aspect ratio approx
             let pos = [box_center_x - w / 2.0, cursor_y];
             draw_list.add_image(*tex_id, pos, [pos[0] + w, pos[1] + h]).col(color).build();
             cursor_y += h + 2.0;
        } else {
             let width = ui.calc_text_size(info.weapon_name)[0];
             let pos = [box_center_x - width / 2.0, cursor_y];
             draw_list.add_text(pos, color, info.weapon_name);
             cursor_y += 21.0;
        }
    }
    ui.set_window_font_scale(1.0);
}