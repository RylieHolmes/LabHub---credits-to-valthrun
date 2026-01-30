// controller/src/enhancements/player/mod.rs

use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use cs2::{
    BoneFlags, CEntityIdentityEx, CS2Model, ClassNameCache, LocalCameraControllerTarget,
    PlayerPawnState, StateCS2Memory, StateEntityList, StateLocalPlayerController, StatePawnInfo,
    StatePawnModelInfo, StatePawnModelAddress, WeaponId
};
use cs2_schema_cutl::EntityHandle;
use cs2_schema_generated::cs2::client::C_BaseEntity;
use cs2_schema_generated::cs2::client::CCSPlayerController;
use imgui::Ui;
use info_layout::{PlayerInfoLayout, LayoutAlignment, ColorContext};
use nalgebra::{Vector3, Matrix4};
use obfstr::obfstr;
use overlay::UnicodeTextRenderer;
use utils_state::StateRegistry;

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
    pawn_handle: u32,
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

fn map_weapon_to_icon(weapon: WeaponId) -> &'static str {
    match weapon {
        WeaponId::KnifeT => "knife_t",
        WeaponId::Knife => "knife",
        WeaponId::KnifeBayonet => "bayonet",
        WeaponId::KnifeButterfly => "knife_butterfly",
        WeaponId::KnifesClassic => "knife_css",
        WeaponId::KnifeCord => "knife_cord",
        WeaponId::KnifeFalchion => "knife_falchion",
        WeaponId::KnifeFlip => "knife_flip",
        WeaponId::KnifeGut => "knife_gut",
        WeaponId::KnifeTactical => "knife_tactical",
        WeaponId::KnifeKarambit => "knife_karambit",
        WeaponId::KnifeM9Bayonet => "knife_m9_bayonet",
        WeaponId::KnifesNavaja => "knife_gypsy_jackknife",
        WeaponId::KnifesNomad => "knife_outdoor",
        WeaponId::KnifePush => "knife_push",
        WeaponId::KnifesSkeleton => "knife_skeleton",
        WeaponId::KnifesStiletto => "knife_stiletto",
        WeaponId::KnifeSurvivalBowie | WeaponId::KnifeSurvival => "knife_survival_bowie",
        WeaponId::KnifesTalon => "knife_widowmaker",
        WeaponId::KnifeUrsus => "knife_ursus",
        WeaponId::Deagle => "deagle",
        WeaponId::Revolver => "revolver",
        WeaponId::CZ75a => "cz75a",
        WeaponId::Elite => "elite",
        WeaponId::HKP200 => "hkp2000",
        WeaponId::Glock => "glock",
        WeaponId::P250 => "p250",
        WeaponId::FiveSeven => "fiveseven",
        WeaponId::Tec9 => "tec9",
        WeaponId::USPS => "usp_silencer",
        WeaponId::M4A1Silencer => "m4a1_silencer",
        WeaponId::M4A4 => "m4a1",
        WeaponId::Ak47 => "ak47",
        WeaponId::Galilar => "galilar",
        WeaponId::Famas => "famas",
        WeaponId::Aug => "aug",
        WeaponId::Sg553 => "sg556",
        WeaponId::Ssg08 => "ssg08",
        WeaponId::AWP => "awp",
        WeaponId::G3SG1 => "g3sg1",
        WeaponId::Scar20 => "scar20",
        WeaponId::Mac10 => "mac10",
        WeaponId::MP5SD => "mp5sd",
        WeaponId::Ump45 => "ump451",
        WeaponId::Bizon => "bizon",
        WeaponId::MP7 => "mp7",
        WeaponId::MP9 => "mp9",
        WeaponId::P90 => "p90",
        WeaponId::Mag7 => "mag7",
        WeaponId::Nova => "nova",
        WeaponId::SawedOff => "sawedoff",
        WeaponId::XM1014 => "xm1014",
        WeaponId::M249 => "m249",
        WeaponId::Negev => "negev",
        WeaponId::Taser => "taser",
        WeaponId::HZgrenade => "hegrenade",
        WeaponId::Smokegrenade => "smokegrenade",
        WeaponId::Flashbang => "flashbang",
        WeaponId::Molotov => "molotov",
        WeaponId::Incendiary => "incgrenade0",
        WeaponId::Decoy => "decoy",
        WeaponId::C4 => "c4",
        _ => "",
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
            let entity_class = class_name_cache.lookup(&entity_identity.entity_class_info()?)?;
            if !entity_class.map(|name| *name == "CCSPlayerController").unwrap_or(false) { continue; }
            
            let controller_handle = entity_identity.handle::<dyn CCSPlayerController>()?;
            let Some(controller_ptr) = entities.entity_from_handle(&controller_handle) else { continue; };
            let Some(controller) = controller_ptr.value_reference(memory.view_arc()) else { continue; };
            
            let pawn_handle = controller.m_hPlayerPawn()?;
            if !pawn_handle.is_valid() { continue; }
            
            let entity_index = pawn_handle.get_entity_index();
            if entity_index == view_target_entity_id { continue; }

            // Only validate player is alive - don't cache any state, we'll read fresh at render time
            let pawn_state = ctx.states.resolve::<PlayerPawnState>(pawn_handle)?;
            if *pawn_state != PlayerPawnState::Alive { continue; }

            valid_player_handles.insert(entity_index);
            self.players.entry(entity_index).or_insert_with(|| PlayerData { 
                pawn_handle: entity_index,
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
        
        // Refresh view matrix to get fresh data at render time (not cached from update phase)
        view.refresh_view_matrix(states)?;

        let camera_position = match view.get_camera_world_position() {
            Some(pos) => pos,
            None => return Ok(())
        };

        let settings = states.resolve::<AppSettings>(())?;
        let app_resources = states.resolve::<AppResources>(()).ok();
        let memory = states.resolve::<StateCS2Memory>(())?;
        let entities = states.resolve::<StateEntityList>(())?;

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
            // READ ALL DATA FRESH AT RENDER TIME - DO NOT USE CACHED DATA FROM UPDATE PHASE
            // This eliminates the 1-frame lag caused by state caching
            
            let pawn_handle_index = entry.pawn_handle;
            
            // Get the entity identity and read fresh pawn info
            let Some(entity_identity) = entities.identity_from_index(pawn_handle_index) else { continue; };
            let Ok(entity_ptr) = entity_identity.entity_ptr::<dyn C_BaseEntity>() else { continue; };
            let Some(entity_ref) = entity_ptr.value_reference(memory.view_arc()) else { continue; };
            
            // Read position directly from the entity
            let interpolated_position = {
                let mut pos = Vector3::new(0.0, 0.0, 0.0);
                if let Ok(game_scene_node) = entity_ref.m_pGameSceneNode() {
                    if let Some(scene_node_ref) = game_scene_node.value_reference(memory.view_arc()) {
                        if let Ok(origin) = scene_node_ref.m_vecAbsOrigin() {
                            pos = Vector3::new(origin[0], origin[1], origin[2]);
                        }
                    }
                }
                pos
            };

            // We need to read player info - try fresh read if possible, otherwise skip
            let pawn_info = match states.resolve::<StatePawnInfo>(EntityHandle::from_index(pawn_handle_index)) {
                Ok(info) => {
                    if info.player_health <= 0 || info.player_name.is_none() { continue; }
                    info
                }
                Err(_) => { continue; }
            };

            let distance = (interpolated_position - camera_position).norm() * UNITS_TO_METERS;

            let esp_settings = match Self::resolve_esp_player_config(&settings, &pawn_info, self.local_team_id) {
                Some(settings) => settings,
                None => continue,
            };

            let player_rel_health = (pawn_info.player_health as f32 / 100.0).clamp(0.0, 1.0);
            
            // Get model data
            let pawn_model_address = match states.resolve::<StatePawnModelAddress>(EntityHandle::from_index(pawn_handle_index)) {
                Ok(addr) => addr.model_address,
                Err(_) => { continue; }
            };
            
            let Ok(entry_model) = states.resolve::<CS2Model>(pawn_model_address) else { continue; };
            
            let player_2d_box = view.calculate_box_2d(
                &(entry_model.vhull_min + interpolated_position),
                &(entry_model.vhull_max + interpolated_position),
            );
            
            let color_ctx = ColorContext { health: player_rel_health, distance, time };

            // --- OFF-SCREEN ARROWS LOGIC (CLIP SPACE METHOD) ---
            if esp_settings.offscreen_arrows {
                let vec = interpolated_position;
                let clip = nalgebra::Vector4::new(vec.x, vec.y, vec.z, 1.0).transpose() * view.view_matrix;
                
                let is_offscreen = if clip.w < 0.1 {
                    true 
                } else {
                    clip.x < -clip.w || clip.x > clip.w || clip.y < -clip.w || clip.y > clip.w
                };

                if is_offscreen {
                    if best_arrow.as_ref().map_or(true, |a| distance < a.dist) {
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
            // Only read bones if actually needed
            let needs_bones = esp_settings.skeleton || esp_settings.chams || esp_settings.head_dot != EspHeadDot::None;
            let pawn_bones = if needs_bones {
                states.resolve::<StatePawnModelInfo>(EntityHandle::from_index(pawn_handle_index)).ok()
            } else {
                None
            };

            if esp_settings.chams {
                if let Some(pawn_model) = &pawn_bones {
                    const MODEL_NAME: &str = "character.glb";
                    if !self.models.contains_key(MODEL_NAME) {
                        let model_name_string = MODEL_NAME.to_string();
                        match CharacterModel::load(MODEL_NAME) {
                            Ok(model) => { self.models.insert(model_name_string, Some(model)); },
                            Err(_) => { self.models.insert(model_name_string, None); }
                        }
                    }

                    if let Some(Some(model)) = self.models.get(MODEL_NAME) {
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
                                let t_bone = (bone_world_pos.z - interpolated_position.z) / 72.0;
                                let col_arr = esp_settings.chams_color.calculate_color(player_rel_health, distance, time, t_bone);
                                let thickness_scale = 600.0; 
                                let dist_clamped = distance.max(0.1);
                                let thickness_mult = if bone.name.contains("spine") || bone.name.contains("pelvis") { 0.35 } else if bone.name.contains("head") { 0.30 } else { 0.15 };
                                draw.add_line([parent_pos.x, parent_pos.y], [bone_pos.x, bone_pos.y], col_arr).thickness((thickness_scale * thickness_mult) / dist_clamped).build();
                            }
                        }
                    }
                }
            }

            if esp_settings.skeleton {
                if let Some(pawn_model) = &pawn_bones {
                    let bones = entry_model.bones.iter().zip(pawn_model.bone_states.iter());
                    for (bone, state) in bones {
                        if (bone.flags & BoneFlags::FlagHitbox as u32) == 0 { continue; }
                        let parent_index = if let Some(parent) = bone.parent { parent } else { continue; };
                        let parent_world_pos = pawn_model.bone_states[parent_index].position;
                        let bone_world_pos = state.position;
                        if let (Some(parent_pos), Some(bone_pos)) = (view.world_to_screen(&parent_world_pos, true), view.world_to_screen(&bone_world_pos, true)) {
                            let t_bone = (bone_world_pos.z - interpolated_position.z) / 72.0;
                            let col = esp_settings.skeleton_color.calculate_color(player_rel_health, distance, time, t_bone);
                            draw.add_line([parent_pos.x, parent_pos.y], [bone_pos.x, bone_pos.y], col).thickness(esp_settings.skeleton_width).build();
                        }
                    }
                }
            }

            if esp_settings.head_dot != EspHeadDot::None {
                if let Some(pawn_model) = &pawn_bones {
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

                if esp_settings.info_name {
                    layout_bottom.add_line(&esp_settings.info_name_color, &color_ctx, pawn_info.player_name.as_ref().map_or("unknown", String::as_str));
                    if let Some(player_name) = &pawn_info.player_name { unicode_text.register_unicode_text(player_name); }
                }

                if esp_settings.info_ammo && pawn_info.weapon_current_ammo != -1 { layout_bottom.add_line(&esp_settings.info_ammo_color, &color_ctx, &format!("{}/{}", pawn_info.weapon_current_ammo, pawn_info.weapon_reserve_ammo)); }
                if esp_settings.info_distance { layout_bottom.add_line(&esp_settings.info_distance_color, &color_ctx, &format!("{:.0}m", distance)); }
                
                if esp_settings.info_weapon {
                    match esp_settings.info_weapon_style {
                        EspInfoStyle::Text => {
                            layout_bottom.add_line(&esp_settings.info_weapon_color, &color_ctx, pawn_info.weapon.display_name());
                        }
                        EspInfoStyle::Icon => {
                            let mut icon_drawn = false;
                            if let Some(resources) = &app_resources {
                                let icon_key = map_weapon_to_icon(pawn_info.weapon);
                                if let Some(tex_id) = resources.weapon_icons.get(icon_key) {
                                    let aspect_ratio = get_weapon_icon_aspect_ratio(icon_key);
                                    let scale = get_weapon_icon_scale(icon_key);
                                    layout_bottom.add_image(*tex_id, &esp_settings.info_weapon_color, &color_ctx, 31.5 * scale, aspect_ratio);
                                    icon_drawn = true;
                                }
                            }
                            if !icon_drawn { layout_bottom.add_line(&esp_settings.info_weapon_color, &color_ctx, pawn_info.weapon.display_name()); }
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
                let arrow_x = screen_center[0] - arrow.radius; 
                let arrow_y = center_y;
                
                let p1 = [arrow_x, arrow_y];
                let p2 = [arrow_x + size, arrow_y - size];
                let p3 = [arrow_x + size, arrow_y + size];

                draw.add_triangle(p1, p2, p3, arrow.color).filled(true).build();
                draw.add_triangle(p1, p2, p3, [0.0, 0.0, 0.0, 1.0]).thickness(1.0).build();
            } else {
                let arrow_x = screen_center[0] + arrow.radius; 
                let arrow_y = center_y;
                
                let p1 = [arrow_x, arrow_y];
                let p2 = [arrow_x - size, arrow_y - size];
                let p3 = [arrow_x - size, arrow_y + size];

                draw.add_triangle(p1, p2, p3, arrow.color).filled(true).build();
                draw.add_triangle(p1, p2, p3, [0.0, 0.0, 0.0, 1.0]).thickness(1.0).build();
            }
        }

        Ok(())
    }
}