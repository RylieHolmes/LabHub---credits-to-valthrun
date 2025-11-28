use anyhow::{Context, Result};
use cs2::{
    CEntityIdentityEx, MouseState, StateCS2Memory, StateEntityList, StateLocalPlayerController,
    StatePawnInfo, StatePawnModelInfo,
};
use cs2_schema_generated::cs2::client::{CCSPlayerController, C_BaseEntity, C_CSPlayerPawn};
use nalgebra::Vector2;
use overlay::UnicodeTextRenderer;
use utils_state::StateRegistry;

use super::Enhancement;
use crate::{settings::AppSettings, view::ViewController, UpdateContext};

pub struct LegitAim {
    // We can store the last target to keep locking on the same person if possible
    // But for a simple legit aim, finding the closest to crosshair every frame is usually fine and feels more natural (switching targets if one gets closer)
}

impl LegitAim {
    pub fn new() -> Self {
        Self {}
    }
}

impl Enhancement for LegitAim {
    fn update(&mut self, ctx: &UpdateContext) -> Result<()> {
        let settings = ctx.states.resolve::<AppSettings>(())?;
        if !settings.legit_aim_enabled {
            return Ok(());
        }

        // Check Key
        if let Some(key) = settings.legit_aim_key {
            if !ctx.input.is_key_down(key.0) {
                return Ok(());
            }
        } else {
            // If no key is set, we probably shouldn't be aiming automatically for "legit" aim.
            return Ok(());
        }

        let memory = ctx.states.resolve::<StateCS2Memory>(())?;
        let entities = ctx.states.resolve::<StateEntityList>(())?;
        let local_controller = ctx.states.resolve::<StateLocalPlayerController>(())?;
        let view = ctx.states.resolve::<ViewController>(())?;

        let Some(local_player_controller) = local_controller
            .instance
            .value_reference(memory.view_arc())
        else {
            return Ok(());
        };

        let local_team = local_player_controller.m_iTeamNum()?;
        let screen_center = Vector2::new(view.screen_bounds.x / 2.0, view.screen_bounds.y / 2.0);

        let mut best_target: Option<(Vector2<f32>, f32)> = None; // (ScreenPos, Distance)
        let max_fov_sq = settings.legit_aim_fov * settings.legit_aim_fov;

        for entity_identity in entities.entities() {
            let handle = entity_identity.handle::<()>()?;
            // Skip if invalid or not a pawn (simple check, can be improved)
            // We rely on StatePawnInfo resolution to filter valid pawns
            
            // Optimization: Check class name cache if available, or just try to resolve PawnInfo
            // Resolving PawnInfo is relatively cheap if cached
            
            // We need the specific C_CSPlayerPawn handle
            let pawn_handle = entity_identity.handle::<dyn C_CSPlayerPawn>()?;

            // Check if it's a valid pawn
            let Ok(pawn_info) = ctx.states.resolve::<StatePawnInfo>(pawn_handle) else {
                continue;
            };

            // Team Check
            if pawn_info.team_id == local_team {
                continue;
            }

            // Alive Check
            if pawn_info.player_health <= 0 {
                continue;
            }

            // Get Bone Position
            // We need StatePawnModelInfo to get bones
            let Ok(pawn_model) = ctx.states.resolve::<StatePawnModelInfo>(pawn_handle) else {
                continue;
            };
            
            // Resolve Model to get bone names
            let Ok(model) = ctx.states.resolve::<cs2::CS2Model>(pawn_model.model_address) else {
                continue;
            };

            // Find the target bone index
            let bone_name = &settings.legit_aim_bone;
            let Some(bone_index) = model.bones.iter().position(|b| b.name == *bone_name) else {
                continue;
            };

            // Get Bone Position
            if let Some(bone_state) = pawn_model.bone_states.get(bone_index) {
                // World to Screen
                // We use the bone position directly. 
                // Note: pawn_info.position is the origin, bone_state.position is world space bone pos.
                // Wait, StatePawnModelInfo implementation:
                // bone_states are read from m_modelState.bone_state_data.
                // These are usually local to the model or world space depending on how they are read.
                // Looking at `cs2/src/state/player.rs`:
                // `BoneStateData` reads `position` from `CBoneStateData`.
                // In CS2, these are usually World Space.
                // Let's assume World Space as `PlayerESP` uses them for lines.
                
                let bone_world_pos = bone_state.position;

                if let Some(screen_pos) = view.world_to_screen(&bone_world_pos, false) {
                    let screen_pos_vec = Vector2::new(screen_pos.x, screen_pos.y);
                    let diff = screen_pos_vec - screen_center;
                    let dist_sq = diff.norm_squared();

                    if dist_sq < max_fov_sq {
                        if best_target.is_none() || dist_sq < best_target.unwrap().1 {
                            best_target = Some((screen_pos_vec, dist_sq));
                        }
                    }
                }
            }
        }

        if let Some((target_screen_pos, _)) = best_target {
            let diff = target_screen_pos - screen_center;
            
            // Smoothing
            // A simple smooth factor: move 1/smooth of the way
            // Ensure smooth is at least 1.0 to avoid overshooting or division by zero
            let smooth = settings.legit_aim_smooth.max(1.0);
            
            let move_x = (diff.x / smooth).round() as i32;
            let move_y = (diff.y / smooth).round() as i32;

            if move_x != 0 || move_y != 0 {
                ctx.cs2.send_mouse_state(&[MouseState {
                    last_x: move_x,
                    last_y: move_y, // MouseState y is usually positive down, same as screen space
                    ..Default::default()
                }])?;
            }
        }

        Ok(())
    }

    fn render(
        &mut self,
        _states: &StateRegistry,
        _ui: &imgui::Ui,
        _unicode_text: &UnicodeTextRenderer,
    ) -> Result<()> {
        Ok(())
    }
}
