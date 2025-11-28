// controller/src/enhancements/player/mod.rs

use utils_state::State;
use std::collections::HashMap;
use std::time::Instant;
use std::f32::consts::PI;

use anyhow::Result;
use cs2::{
    BoneFlags, CEntityIdentityEx, CS2Model, ClassNameCache, LocalCameraControllerTarget,
    PlayerPawnState, StateCS2Memory, StateEntityList, StateLocalPlayerController, StatePawnInfo,
    StatePawnModelInfo,
};
use cs2_schema_generated::cs2::client::C_CSPlayerPawn;
use imgui::Ui;
use info_layout::{PlayerInfoLayout, LayoutAlignment, ColorContext};
use nalgebra::{Vector3, Matrix4, UnitQuaternion};
use obfstr::obfstr;
use overlay::UnicodeTextRenderer;
use utils_state::StateRegistry;

use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

use super::Enhancement;
use crate::{
    settings::{
        AppSettings, EspBoxType, EspConfig, EspHeadDot, EspHealthBar, EspPlayerSettings,
        EspSelector, EspTracePosition, EspInfoStyle, EspColor,
    },
    view::{KeyToggle, ViewController},
    AppResources,
};

mod info_layout;
pub mod model_renderer;
use model_renderer::CharacterModel;

struct PlayerData {
    pawn_info: StatePawnInfo,
    pawn_model: StatePawnModelInfo,
    previous_position: Vector3<f32>,
    current_position: Vector3<f32>,
    last_update_time: Instant,
    bone_transforms: HashMap<String, Matrix4<f32>>,
}

pub struct PlayerESP {
    toggle: KeyToggle,
    players: HashMap<u32, PlayerData>,
    local_team_id: u8,
    start_time: Instant,
    models: HashMap<String, Option<CharacterModel>>,
}

fn lerp(start: Vector3<f32>, end: Vector3<f32>, t: f32) -> Vector3<f32> {
    start + (end - start) * t
}

fn map_weapon_to_icon(display_name: &str) -> String {
    let lower = display_name.to_lowercase();
    match lower.as_str() {
        "knife (t)" => "knife_t".to_string(),
        "knife" => "knife".to_string(),
        "bayonet" => "bayonet".to_string(),
        "butterfly knife" => "knife_butterfly".to_string(),
        "classic knife" => "knife_css".to_string(),
        "cord knife" => "knife_cord".to_string(),
        "falchion knife" => "knife_falchion".to_string(),
        "flip knife" => "knife_flip".to_string(),
        "gut knife" => "knife_gut".to_string(),
        "huntsman knife" => "knife_tactical".to_string(),
        "karambit" => "knife_karambit".to_string(),
        "m9 bayonet" => "knife_m9_bayonet".to_string(),
        "navaja knife" => "knife_gypsy_jackknife".to_string(),
        "nomad knife" => "knife_outdoor".to_string(),
        "shadow daggers" => "knife_push".to_string(),
        "skeleton knife" => "knife_skeleton".to_string(),
        "stiletto knife" => "knife_stiletto".to_string(),
        "survival knife" => "knife_survival_bowie".to_string(),
        "talon knife" => "knife_widowmaker".to_string(),
        "ursus knife" => "knife_ursus".to_string(),
        "bowie knife" => "knife_survival_bowie".to_string(),
        "desert eagle" => "deagle".to_string(),
        "r8 revolver" => "revolver".to_string(),
        "cz75-auto" => "cz75a".to_string(),
        "dual berettas" => "elite".to_string(),
        "p2000" => "hkp2000".to_string(),
        "glock-18" => "glock".to_string(),
        "p250" => "p250".to_string(),
        "five-seven" => "fiveseven".to_string(),
        "tec-9" => "tec9".to_string(),
        "usp-s" => "usp_silencer".to_string(),
        "m4a1-s" => "m4a1_silencer".to_string(),
        "m4a4" => "m4a1".to_string(),
        "ak-47" => "ak47".to_string(),
        "galil ar" => "galilar".to_string(),
        "famas" => "famas".to_string(),
        "aug" => "aug".to_string(),
        "sg 553" => "sg556".to_string(),
        "ssg 08" => "ssg08".to_string(),
        "awp" => "awp".to_string(),
        "g3sg1" => "g3sg1".to_string(),
        "scar-20" => "scar20".to_string(),
        "mac-10" => "mac10".to_string(),
        "mp5-sd" => "mp5sd".to_string(),
        "ump-45" => "ump451".to_string(),
        "pp-bizon" => "bizon".to_string(),
        "mp7" => "mp7".to_string(),
        "mp9" => "mp9".to_string(),
        "p90" => "p90".to_string(),
        "mag-7" => "mag7".to_string(),
        "nova" => "nova".to_string(),
        "sawed-off" => "sawedoff".to_string(),
        "xm1014" => "xm1014".to_string(),
        "m249" => "m249".to_string(),
        "negev" => "negev".to_string(),
        "zeus x27" => "taser".to_string(),
        "high explosive grenade" | "he grenade" => "hegrenade".to_string(),
        "smoke grenade" => "smokegrenade".to_string(),
        "flashbang" => "flashbang".to_string(),
        "molotov" => "molotov".to_string(),
        "incendiary grenade" => "incgrenade0".to_string(),
        "decoy grenade" => "decoy".to_string(),
        "c4 explosive" => "c4".to_string(),
        _ => lower.replace(|c: char| !c.is_alphanumeric(), ""),
    }
}

fn get_weapon_icon_aspect_ratio(icon_key: &str) -> f32 {
    match icon_key {
        "hegrenade" | "smokegrenade" | "flashbang" | "molotov" | "incgrenade0" | "decoy" => 0.6,
        _ => 2.5,
    }
}

fn get_weapon_icon_scale(icon_key: &str) -> f32 {
    match icon_key {
        "hegrenade" | "smokegrenade" | "flashbang" | "molotov" | "incgrenade0" | "decoy" => 1.5,
        "c4" => 1.2,
        _ => 1.0,
    }
}

impl PlayerESP {
    pub fn new() -> Self {
        PlayerESP {
            toggle: KeyToggle::new(),
            players: HashMap::new(),
            local_team_id: 0,
            start_time: Instant::now(),
            models: HashMap::new(),
        }
    }
    
    fn resolve_esp_player_config<'a>(
        settings: &'a AppSettings,
        target: &StatePawnInfo,
        local_team_id: u8,
    ) -> Option<&'a EspPlayerSettings> {
        let mut esp_target = Some(EspSelector::PlayerTeamVisibility {
            enemy: target.team_id != local_team_id,
            visible: true,
        });
        while let Some(target) = esp_target.take() {
            let config_key = target.config_key();
            if settings.esp_settings_enabled.get(&config_key).cloned().unwrap_or_default() {
                if let Some(settings) = settings.esp_settings.get(&config_key) {
                    if let EspConfig::Player(settings) = settings { return Some(settings); }
                }
            }
            esp_target = target.parent();
        }
        None
    }
}

impl Enhancement for PlayerESP {
    fn update(&mut self, ctx: &crate::UpdateContext) -> Result<()> {
        let settings = ctx.states.resolve::<AppSettings>(())?;
        if self.toggle.update(&settings.esp_mode, ctx.input, &settings.esp_toggle) {
            ctx.cs2.add_metrics_record(obfstr!("feature-esp-toggle"), &format!("enabled: {}, mode: {:?}", self.toggle.enabled, settings.esp_mode));
        }
        if !self.toggle.enabled { self.players.clear(); return Ok(()); }

        let entities = ctx.states.resolve::<StateEntityList>(())?;
        let class_name_cache = ctx.states.resolve::<ClassNameCache>(())?;
        let memory = ctx.states.resolve::<StateCS2Memory>(())?;
        let local_player_controller = ctx.states.resolve::<StateLocalPlayerController>(())?;

        let Some(local_player_controller) = local_player_controller.instance.value_reference(memory.view_arc()) else { self.players.clear(); return Ok(()); };
        self.local_team_id = local_player_controller.m_iPendingTeamNum()?;
        
        let view_target = ctx.states.resolve::<LocalCameraControllerTarget>(())?;
        let view_target_entity_id = match &view_target.target_entity_id { Some(value) => *value, None => { self.players.clear(); return Ok(()); } };

        let mut valid_player_handles = std::collections::HashSet::new();

        for entity_identity in entities.entities() {
            let handle = entity_identity.handle::<dyn C_CSPlayerPawn>()?;
            let entity_index = handle.get_entity_index();

            if entity_index == view_target_entity_id { continue; }

            let entity_class = class_name_cache.lookup(&entity_identity.entity_class_info()?)?;
            if !entity_class.map(|name| *name == "C_CSPlayerPawn").unwrap_or(false) { continue; }
            let pawn_state = ctx.states.resolve::<PlayerPawnState>(handle)?;
            if *pawn_state != PlayerPawnState::Alive { continue; }
            let pawn_info = ctx.states.resolve::<StatePawnInfo>(handle)?;
            if pawn_info.player_health <= 0 || pawn_info.player_name.is_none() { continue; }
            let Ok(pawn_model) = ctx.states.resolve::<StatePawnModelInfo>(handle) else { continue; };

            valid_player_handles.insert(entity_index);
            let now = Instant::now();
            self.players.entry(entity_index).and_modify(|entry| { 
                entry.previous_position = entry.current_position; 
                entry.current_position = pawn_info.position; 
                entry.pawn_info = pawn_info.clone(); 
                entry.pawn_model = pawn_model.clone(); 
                entry.last_update_time = now; 
            }).or_insert_with(|| PlayerData { 
                previous_position: pawn_info.position, 
                current_position: pawn_info.position, 
                pawn_info: pawn_info.clone(), 
                pawn_model: pawn_model.clone(), 
                last_update_time: now,
                bone_transforms: HashMap::new(),
            });
        }
        self.players.retain(|entity_index, _| valid_player_handles.contains(entity_index));
        Ok(())
    }

    fn render(
        &mut self,
        states: &StateRegistry,
        ui: &Ui,
        unicode_text: &UnicodeTextRenderer,
    ) -> Result<()> {
        if !self.toggle.enabled { return Ok(()); }
        
        let Ok(mut view) = states.resolve_mut::<ViewController>(()) else { return Ok(()); };
        (*view).update(states)?;
        
        let camera_position = match view.get_camera_world_position() {
            Some(pos) => pos,
            None => return Ok(())
        };

        let settings = states.resolve::<AppSettings>(())?;
        let app_resources = states.resolve::<AppResources>(()).ok();

        let draw = ui.get_window_draw_list();
        const UNITS_TO_METERS: f32 = 0.01905;
        const MAX_HEAD_SIZE: f32 = 250.0;
        
        let time = self.start_time.elapsed().as_secs_f32();
        let screen_center = [view.screen_bounds.x / 2.0, view.screen_bounds.y / 2.0];

        // --- SINGLE ARROW STATE TRACKING ---
        struct ClosestArrowState {
            dist: f32,
            color: [f32; 4],
            radius: f32,
            size: f32,
            is_left: bool,
        }
        let mut best_arrow: Option<ClosestArrowState> = None;
        // -----------------------------------

        for (_entity_index, entry) in self.players.iter_mut() {
            let pawn_info = &entry.pawn_info;
            let pawn_model = &entry.pawn_model;
            let interpolated_position = entry.current_position;

            let distance = (interpolated_position - camera_position).norm() * UNITS_TO_METERS;

            let esp_settings = match Self::resolve_esp_player_config(&settings, pawn_info, self.local_team_id) {
                Some(settings) => settings,
                None => continue,
            };

            let player_rel_health = (pawn_info.player_health as f32 / 100.0).clamp(0.0, 1.0);
            let Ok(entry_model) = states.resolve::<CS2Model>(pawn_model.model_address) else { continue; };
            
            let player_2d_box = view.calculate_box_2d(
                &(entry_model.vhull_min + interpolated_position),
                &(entry_model.vhull_max + interpolated_position),
            );
            
            let color_ctx = ColorContext { health: player_rel_health, distance, time };

            // --- OFF-SCREEN ARROWS LOGIC (CLIP SPACE METHOD) ---
            if esp_settings.offscreen_arrows {
                // Manual projection to check "Offscreen-ness" accurately
                let vec = interpolated_position;
                // Use the view matrix from public field
                let clip = nalgebra::Vector4::new(vec.x, vec.y, vec.z, 1.0).transpose() * view.view_matrix;
                
                // Check if offscreen
                // It is offscreen if:
                // 1. Behind camera (w < 0.1)
                // 2. Outside NDC bounds (abs(x) > w or abs(y) > w)
                let is_offscreen = if clip.w < 0.1 {
                    true 
                } else {
                    clip.x < -clip.w || clip.x > clip.w || clip.y < -clip.w || clip.y > clip.w
                };

                if is_offscreen {
                    if best_arrow.as_ref().map_or(true, |a| distance < a.dist) {
                        // Determine Left/Right based on Clip Space X
                        // In standard View Space (and assuming standard Projection matrix):
                        // x < 0 is Left, x > 0 is Right.
                        // This holds true even if w < 0 (behind), because the lateral side doesn't flip.
                        
                        let is_left = clip.x < 0.0; 

                        let color = esp_settings.offscreen_arrows_color.calculate_color(player_rel_health, distance, time, 0.0);
                        
                        best_arrow = Some(ClosestArrowState {
                            dist: distance,
                            color,
                            radius: esp_settings.offscreen_arrows_radius,
                            size: esp_settings.offscreen_arrows_size,
                            is_left,
                        });
                    }
                }
            }
            // ---------------------------------------------------

            // --- MODEL RENDERING START ---
            if esp_settings.chams {
                let model_name = "character.glb".to_string(); 
                if !self.models.contains_key(&model_name) {
                    match CharacterModel::load(&model_name) {
                        Ok(model) => { self.models.insert(model_name.clone(), Some(model)); },
                        Err(_) => { self.models.insert(model_name.clone(), None); }
                    }
                }

                if let Some(Some(model)) = self.models.get(&model_name) {
                    let bones_iter = entry_model.bones.iter().zip(pawn_model.bone_states.iter());
                    for (bone, state) in bones_iter {
                        let bone_pos = state.position;
                        let bone_rot = nalgebra::UnitQuaternion::from_quaternion(state.rotation);
                        let transform = Matrix4::new_translation(&bone_pos) * Matrix4::from(bone_rot);
                        if let Some(t) = entry.bone_transforms.get_mut(&bone.name) { *t = transform; } 
                        else { entry.bone_transforms.insert(bone.name.clone(), transform); }
                    }
                    let col_arr = esp_settings.chams_color.calculate_color(player_rel_health, distance, time, 0.0);
                    model.render(&draw, &view, &entry.bone_transforms, col_arr);
                } else {
                    let bones = entry_model.bones.iter().zip(pawn_model.bone_states.iter());
                    for (bone, state) in bones {
                        if (bone.flags & BoneFlags::FlagHitbox as u32) == 0 { continue; }
                        let parent_index = if let Some(parent) = bone.parent { parent } else { continue; };
                        let parent_world_pos = pawn_model.bone_states[parent_index].position;
                        let bone_world_pos = state.position;
                        if let (Some(parent_pos), Some(bone_pos)) = (view.world_to_screen(&parent_world_pos, true), view.world_to_screen(&bone_world_pos, true)) {
                            let t_bone = (bone_world_pos.z - entry.pawn_info.position.z) / 72.0;
                            let col_arr = esp_settings.chams_color.calculate_color(player_rel_health, distance, time, t_bone);
                            let thickness_scale = 600.0; 
                            let dist_clamped = distance.max(0.1);
                            let thickness_mult = if bone.name.contains("spine") || bone.name.contains("pelvis") { 0.35 } else if bone.name.contains("head") { 0.30 } else { 0.15 };
                            draw.add_line([parent_pos.x, parent_pos.y], [bone_pos.x, bone_pos.y], col_arr).thickness((thickness_scale * thickness_mult) / dist_clamped).build();
                        }
                    }
                }
            }

            if esp_settings.skeleton {
                let bones = entry_model.bones.iter().zip(pawn_model.bone_states.iter());
                for (bone, state) in bones {
                    if (bone.flags & BoneFlags::FlagHitbox as u32) == 0 { continue; }
                    let parent_index = if let Some(parent) = bone.parent { parent } else { continue; };
                    let parent_world_pos = pawn_model.bone_states[parent_index].position;
                    let bone_world_pos = state.position;
                    if let (Some(parent_pos), Some(bone_pos)) = (view.world_to_screen(&parent_world_pos, true), view.world_to_screen(&bone_world_pos, true)) {
                        let t_bone = (bone_world_pos.z - entry.pawn_info.position.z) / 72.0;
                        let col = esp_settings.skeleton_color.calculate_color(player_rel_health, distance, time, t_bone);
                        draw.add_line([parent_pos.x, parent_pos.y], [bone_pos.x, bone_pos.y], col).thickness(esp_settings.skeleton_width).build();
                    }
                }
            }

            if esp_settings.head_dot != EspHeadDot::None {
                if let Some(head_bone_index) = entry_model.bones.iter().position(|bone| bone.name == "head_0") {
                    if let Some(head_state) = pawn_model.bone_states.get(head_bone_index) {
                        let head_base_pos = head_state.position;
                        if let (Some(head_position), Some(head_far)) = (
                            view.world_to_screen(&(head_base_pos + nalgebra::Vector3::new(0.0, 0.0, esp_settings.head_dot_z)), true),
                            view.world_to_screen(&(head_base_pos + nalgebra::Vector3::new(0.0, 0.0, esp_settings.head_dot_z + 2.0)), true),
                        ) {
                            let color = esp_settings.head_dot_color.calculate_color(player_rel_health, distance, time, 0.0);
                            let radius = f32::min(f32::abs(head_position.y - head_far.y), MAX_HEAD_SIZE) * esp_settings.head_dot_base_radius;
                            let circle = draw.add_circle([head_position.x, head_position.y], radius, color);
                            match esp_settings.head_dot {
                                EspHeadDot::Filled => { circle.filled(true).build(); }
                                EspHeadDot::NotFilled => { circle.filled(false).thickness(esp_settings.head_dot_thickness).build(); }
                                EspHeadDot::None => unreachable!(),
                            }
                        }
                    }
                }
            }

            match esp_settings.box_type {
                EspBoxType::Box2D => {
                    if let Some((vmin, vmax)) = &player_2d_box {
                        if let EspColor::GradientVertical { top, bottom } = esp_settings.box_color {
                            let c_top = top.as_f32(); let c_bot = bottom.as_f32();
                             draw.add_rect_filled_multicolor([vmin.x - esp_settings.box_width/2.0, vmin.y], [vmin.x + esp_settings.box_width/2.0, vmax.y], c_top, c_top, c_bot, c_bot);
                             draw.add_rect_filled_multicolor([vmax.x - esp_settings.box_width/2.0, vmin.y], [vmax.x + esp_settings.box_width/2.0, vmax.y], c_top, c_top, c_bot, c_bot);
                             draw.add_rect([vmin.x, vmin.y], [vmax.x, vmin.y + esp_settings.box_width], c_top).filled(true).build();
                             draw.add_rect([vmin.x, vmax.y - esp_settings.box_width], [vmax.x, vmax.y], c_bot).filled(true).build();
                        } else {
                            let col = esp_settings.box_color.calculate_color(player_rel_health, distance, time, 0.0);
                            draw.add_rect([vmin.x, vmin.y], [vmax.x, vmax.y], col).thickness(esp_settings.box_width).build();
                        }
                    }
                }
                EspBoxType::Box3D => {
                    view.draw_box_3d(&draw, &(entry_model.vhull_min + interpolated_position), &(entry_model.vhull_max + interpolated_position), esp_settings.box_color.calculate_color(player_rel_health, distance, time, 0.0).into(), esp_settings.box_width);
                }
                EspBoxType::None => {}
            }

            if let Some((vmin, vmax)) = &player_2d_box {
                let box_bounds = match esp_settings.health_bar {
                    EspHealthBar::None => None,
                    EspHealthBar::Left => Some([vmin.x - esp_settings.box_width / 2.0 - esp_settings.health_bar_width, vmin.y - esp_settings.box_width / 2.0, esp_settings.health_bar_width, vmax.y - vmin.y + esp_settings.box_width]),
                    EspHealthBar::Right => Some([vmax.x + esp_settings.box_width / 2.0, vmin.y - esp_settings.box_width / 2.0, esp_settings.health_bar_width, vmax.y - vmin.y + esp_settings.box_width]),
                    EspHealthBar::Top => Some([vmin.x - esp_settings.box_width / 2.0, vmin.y - esp_settings.box_width / 2.0 - esp_settings.health_bar_width, vmax.x - vmin.x + esp_settings.box_width, esp_settings.health_bar_width]),
                    EspHealthBar::Bottom => Some([vmin.x - esp_settings.box_width / 2.0, vmax.y + esp_settings.box_width / 2.0, vmax.x - vmin.x + esp_settings.box_width, esp_settings.health_bar_width]),
                };

                if let Some([mut box_x, mut box_y, mut box_width, mut box_height]) = box_bounds {
                    const BORDER_WIDTH: f32 = 1.0;
                    draw.add_rect([box_x + BORDER_WIDTH / 2.0, box_y + BORDER_WIDTH / 2.0], [box_x + box_width - BORDER_WIDTH / 2.0, box_y + box_height - BORDER_WIDTH / 2.0], [0.0, 0.0, 0.0, 1.0]).filled(false).thickness(BORDER_WIDTH).build();
                    box_x += BORDER_WIDTH / 2.0 + 1.0; box_y += BORDER_WIDTH / 2.0 + 1.0; box_width -= BORDER_WIDTH + 2.0; box_height -= BORDER_WIDTH + 2.0;
                    if box_width < box_height {
                        let yoffset = box_y + (1.0 - player_rel_health) * box_height;
                        draw.add_rect([box_x, box_y], [box_x + box_width, yoffset], [1.0, 0.0, 0.0, 1.0]).filled(true).build();
                        draw.add_rect([box_x, yoffset], [box_x + box_width, box_y + box_height], esp_settings.info_hp_text_color.calculate_color(player_rel_health, distance, time, 0.5)).filled(true).build();
                    } else {
                        let xoffset = box_x + (1.0 - player_rel_health) * box_width;
                        draw.add_rect([box_x, box_y], [xoffset, box_y + box_height], [1.0, 0.0, 0.0, 1.0]).filled(true).build();
                        draw.add_rect([xoffset, box_y], [box_x + box_width, box_y + box_height], esp_settings.info_hp_text_color.calculate_color(player_rel_health, distance, time, 0.5)).filled(true).build();
                    }
                }
            }

            if let Some((vmin, vmax)) = player_2d_box {
                let mut layout_right = PlayerInfoLayout::new(ui, &draw, view.screen_bounds, vmin, vmax, esp_settings.box_type == EspBoxType::Box2D, LayoutAlignment::Right, esp_settings.text_style);
                let mut layout_bottom = PlayerInfoLayout::new(ui, &draw, view.screen_bounds, vmin, vmax, esp_settings.box_type == EspBoxType::Box2D, LayoutAlignment::Bottom, esp_settings.text_style);

                if esp_settings.info_name {
                    layout_right.add_line(&esp_settings.info_name_color, &color_ctx, pawn_info.player_name.as_ref().map_or("unknown", String::as_str));
                    if let Some(player_name) = &pawn_info.player_name { unicode_text.register_unicode_text(player_name); }
                }
                if esp_settings.info_hp_text { layout_right.add_line(&esp_settings.info_hp_text_color, &color_ctx, &format!("{} HP", pawn_info.player_health)); }
                if esp_settings.info_flag_kit && pawn_info.player_has_defuser { layout_right.add_line(&esp_settings.info_flag_kit_color, &color_ctx, "Kit"); }
                if esp_settings.info_flag_bomb && pawn_info.player_has_bomb { layout_right.add_line(&esp_settings.info_flag_bomb_color, &color_ctx, "Bomb Carrier"); }
                if esp_settings.info_flag_scoped && pawn_info.player_is_scoped { layout_right.add_line(&esp_settings.info_flag_scoped_color, &color_ctx, "Scoped"); }
                if esp_settings.info_flag_flashed && pawn_info.player_flashtime > 0.0 { layout_right.add_line(&esp_settings.info_flag_flashed_color, &color_ctx, "Flashed"); }
                
                let mut player_utilities = Vec::new();
                if esp_settings.info_grenades {
                    if pawn_info.player_has_flash > 0 { player_utilities.push(format!("Flashbang x{}", pawn_info.player_has_flash)); }
                    if pawn_info.player_has_smoke { player_utilities.push("Smoke".to_string()); }
                    if pawn_info.player_has_hegrenade { player_utilities.push("HE Grenade".to_string()); }
                    if pawn_info.player_has_molotov { player_utilities.push("Molotov".to_string()); }
                    if pawn_info.player_has_incendiary { player_utilities.push("Incendiary".to_string()); }
                    if pawn_info.player_has_decoy { player_utilities.push("Decoy".to_string()); }
                    if !player_utilities.is_empty() { layout_right.add_line(&esp_settings.info_grenades_color, &color_ctx, &player_utilities.join(", ")); }
                }

                if esp_settings.info_ammo && pawn_info.weapon_current_ammo != -1 { layout_bottom.add_line(&esp_settings.info_ammo_color, &color_ctx, &format!("{}/{}", pawn_info.weapon_current_ammo, pawn_info.weapon_reserve_ammo)); }
                if esp_settings.info_distance { layout_bottom.add_line(&esp_settings.info_distance_color, &color_ctx, &format!("{:.0}m", distance)); }
                
                if esp_settings.info_weapon {
                    let weapon_name = pawn_info.weapon.display_name();
                    match esp_settings.info_weapon_style {
                        EspInfoStyle::Text => {
                            layout_bottom.add_line(&esp_settings.info_weapon_color, &color_ctx, weapon_name);
                        }
                        EspInfoStyle::Icon => {
                            let mut icon_drawn = false;
                            if let Some(resources) = &app_resources {
                                let icon_key = map_weapon_to_icon(weapon_name);
                                if let Some(tex_id) = resources.weapon_icons.get(&icon_key) {
                                    let aspect_ratio = get_weapon_icon_aspect_ratio(&icon_key);
                                    let scale = get_weapon_icon_scale(&icon_key);
                                    layout_bottom.add_image(*tex_id, &esp_settings.info_weapon_color, &color_ctx, 31.5 * scale, aspect_ratio);
                                    icon_drawn = true;
                                }
                            }
                            if !icon_drawn { layout_bottom.add_line(&esp_settings.info_weapon_color, &color_ctx, weapon_name); }
                        }
                    }
                }
            }

            if let Some(pos) = view.world_to_screen(&interpolated_position, false) {
                let tracer_origin = match esp_settings.tracer_lines {
                    EspTracePosition::TopLeft => Some([0.0, 0.0]),
                    EspTracePosition::TopCenter => Some([view.screen_bounds.x / 2.0, 0.0]),
                    EspTracePosition::TopRight => Some([view.screen_bounds.x, 0.0]),
                    EspTracePosition::Center => Some([view.screen_bounds.x / 2.0, view.screen_bounds.y / 2.0]),
                    EspTracePosition::BottomLeft => Some([0.0, view.screen_bounds.y]),
                    EspTracePosition::BottomCenter => Some([view.screen_bounds.x / 2.0, view.screen_bounds.y]),
                    EspTracePosition::BottomRight => Some([view.screen_bounds.x, view.screen_bounds.y]),
                    EspTracePosition::None => None,
                };
                if let Some(origin) = tracer_origin {
                    draw.add_line([origin[0], origin[1]], [pos.x, pos.y], esp_settings.tracer_lines_color.calculate_color(player_rel_health, distance, time, 0.0)).thickness(esp_settings.tracer_lines_width).build();
                }
            }
        }

        // --- DRAW SINGLE AGGREGATED OFFSCREEN ARROW ---
        if let Some(arrow) = best_arrow {
            let center_y = screen_center[1];
            let size = arrow.size;
            
            if arrow.is_left {
                // Draw Left Arrow
                let arrow_x = screen_center[0] - arrow.radius; 
                let arrow_y = center_y;
                
                let p1 = [arrow_x, arrow_y]; // Tip
                let p2 = [arrow_x + size, arrow_y - size]; // Base Top
                let p3 = [arrow_x + size, arrow_y + size]; // Base Bot

                draw.add_triangle(p1, p2, p3, arrow.color).filled(true).build();
                draw.add_triangle(p1, p2, p3, [0.0, 0.0, 0.0, 1.0]).thickness(1.0).build();
            } else {
                // Draw Right Arrow
                let arrow_x = screen_center[0] + arrow.radius; 
                let arrow_y = center_y;
                
                let p1 = [arrow_x, arrow_y]; // Tip
                let p2 = [arrow_x - size, arrow_y - size]; // Base Top
                let p3 = [arrow_x - size, arrow_y + size]; // Base Bot

                draw.add_triangle(p1, p2, p3, arrow.color).filled(true).build();
                draw.add_triangle(p1, p2, p3, [0.0, 0.0, 0.0, 1.0]).thickness(1.0).build();
            }
        }
        // ----------------------------------------------

        Ok(())
    }
}