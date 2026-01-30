use anyhow::Result;
use crate::enhancements::Enhancement;
use crate::UpdateContext;
use utils_state::StateRegistry;
use crate::settings::AppSettings;
use cs2_schema_generated::cs2::client::{
    C_CSPlayerPawn, 
    C_EconEntity,
    C_BaseModelEntity, 
    C_BaseEntity, // Required for m_vecAbsVelocity
};
use cs2::{
    StateCS2Memory, 
    StateEntityList, 
    StateLocalPlayerController,
    StatePawnInfo,
    StateCurrentMap,
    WeaponId,
    WEAPON_FLAG_TYPE_GRENADE,
};
use imgui::Ui;
use overlay::UnicodeTextRenderer;
use nalgebra::{Vector3, Unit};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
use crate::enhancements::map_loader::MapMesh;
use crate::view::ViewController;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ActiveGrenadeType {
    Smoke,
    Molotov,
    HE,
    Flash,
    Decoy,
    Unknown,
}

impl ActiveGrenadeType {
    fn get_visuals(&self) -> ([f32; 4], f32) {
        match self {
            Self::Smoke => ([0.5, 0.5, 0.6, 0.4], 144.0),   
            Self::Molotov => ([1.0, 0.3, 0.0, 0.4], 150.0), 
            Self::HE => ([1.0, 0.1, 0.1, 0.4], 144.0),      
            Self::Flash => ([1.0, 1.0, 1.0, 0.6], 30.0),    
            Self::Decoy => ([0.2, 1.0, 0.2, 0.6], 15.0),    
            Self::Unknown => ([1.0, 1.0, 1.0, 0.4], 10.0),
        }
    }

    fn get_detonation_time(&self) -> Option<f32> {
        match self {
            Self::HE => Some(1.1),
            Self::Flash => Some(1.1), 
            Self::Molotov => Some(3.5), // Max air time
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct TrajectoryState {
    position: Vector3<f32>,
    velocity: Vector3<f32>,
    throw_strength: f32,
    weapon_id: WeaponId,
}

impl TrajectoryState {
    fn is_similar(&self, other: &Self) -> bool {
        if self.weapon_id != other.weapon_id { return false; }
        if (self.throw_strength - other.throw_strength).abs() > 0.01 { return false; }
        if (self.position - other.position).norm_squared() > 0.01 { return false; }
        if (self.velocity - other.velocity).norm_squared() > 0.01 { return false; }
        true
    }
}

pub struct GrenadeTrajectory {
    trajectory: Option<Vec<Vector3<f32>>>,
    active_type: ActiveGrenadeType,
    map_mesh: Option<MapMesh>,
    current_map_name: Option<String>,
    debug_draw_mesh: bool, 
    last_calc_state: Option<TrajectoryState>,
    last_logged_wid: Option<WeaponId>,
}

struct TraceResult {
    fraction: f32,
    did_hit: bool,
    plane_normal: Vector3<f32>,
    end_pos: Vector3<f32>,
}

impl GrenadeTrajectory {
    pub fn new() -> Self {
        // Don't load map in new(), load it in update() based on current map name
        Self { 
            trajectory: None,
            active_type: ActiveGrenadeType::Unknown,
            map_mesh: None,
            current_map_name: None,
            debug_draw_mesh: false, 
            last_calc_state: None,
            last_logged_wid: None,
        }
    }

    fn trace_ray(&self, start: Vector3<f32>, end: Vector3<f32>) -> TraceResult {
        // 1. Check Map Geometry
        if let Some(mesh) = &self.map_mesh {
            // Use radius 2.0 for grenade collision (approximate size)
            if let Some((fraction, hit_pos, normal)) = mesh.check_collision(start, end, 2.0) {
                return TraceResult {
                    fraction,
                    did_hit: true,
                    plane_normal: normal,
                    end_pos: hit_pos,
                };
            }
        }

        // No fallback floor collision - let it fall freely if no map hit
        TraceResult {
            fraction: 1.0,
            did_hit: false,
            plane_normal: Vector3::new(0.0, 0.0, 0.0),
            end_pos: end,
        }
    }

    fn physics_clip_velocity(&self, input_vel: Vector3<f32>, normal: Vector3<f32>, overbounce: f32) -> Vector3<f32> {
        let backoff = input_vel.dot(&normal) * overbounce;
        let change = normal * backoff;
        let mut out_vel = input_vel - change;
        
        let adjust = 0.1;
        if out_vel.x > -adjust && out_vel.x < adjust { out_vel.x = 0.0; }
        if out_vel.y > -adjust && out_vel.y < adjust { out_vel.y = 0.0; }
        if out_vel.z > -adjust && out_vel.z < adjust { out_vel.z = 0.0; }
        out_vel
    }
}

impl Enhancement for GrenadeTrajectory {
    fn update(&mut self, ctx: &UpdateContext) -> Result<()> {
        let settings = ctx.states.resolve::<AppSettings>(())?;
        if !settings.grenade_trajectory.enabled {
            self.trajectory = None;
            return Ok(());
        }

        // Map Loading Logic
        // Map Loading Logic
        let selected_map = &settings.grenade_trajectory.selected_map;
        
        let target_map = if selected_map != "Auto" {
            Some(selected_map.clone())
        } else {
            // Auto mode
            if let Ok(map_state) = ctx.states.resolve::<StateCurrentMap>(()) {
                map_state.current_map.clone()
            } else {
                None
            }
        };

        // Determine final map to try loading (with fallback)
        let final_map = target_map.or_else(|| {
             // Only fallback if we haven't loaded ANYTHING yet
             if self.current_map_name.is_none() {
                 Some("de_mirage".to_string())
             } else {
                 None
             }
        });

        if let Some(map_name) = final_map {
             if map_name != "<empty>" && self.current_map_name.as_ref() != Some(&map_name) {
                 log::info!("Map switch requested: {}", map_name);
                 self.current_map_name = Some(map_name.clone());
                 
                 let resources_path = format!("resources/{}.glb", map_name);
                 let cwd_path = format!("{}.glb", map_name);
                 
                 let glb_path = if std::path::Path::new(&resources_path).exists() {
                     resources_path
                 } else {
                     cwd_path
                 };
                 match MapMesh::load(&glb_path) {
                     Ok(mesh) => {
                         log::info!("Loaded collision mesh: {}", glb_path);
                         self.map_mesh = Some(mesh);
                     },
                     Err(e) => {
                         log::warn!("Failed to load collision mesh for {}: {:#}", map_name, e);
                         self.map_mesh = None;
                     }
                 }
             }
        }


        let memory = ctx.states.resolve::<StateCS2Memory>(())?;
        let entities = ctx.states.resolve::<StateEntityList>(())?;
        let local_player_controller = ctx.states.resolve::<StateLocalPlayerController>(())?;

        let Some(local_controller) = local_player_controller.instance.value_reference(memory.view_arc()) else { 
            self.trajectory = None; return Ok(()); 
        };
        
        let local_pawn_handle = match local_controller.m_hPlayerPawn() {
            Ok(h) => h,
            Err(_) => { self.trajectory = None; return Ok(()); }
        };

        let Some(local_pawn_entity) = entities.entity_from_handle(&local_pawn_handle) else { 
            self.trajectory = None; return Ok(()); 
        };
        
        let local_pawn_ptr = local_pawn_entity.cast::<dyn C_CSPlayerPawn>();
        let Some(local_pawn) = local_pawn_ptr.value_reference(memory.view_arc()) else { 
            self.trajectory = None; return Ok(()); 
        };

        let weapon_id = (|| -> anyhow::Result<Option<WeaponId>> {
            let weapon_handle = local_pawn.m_pClippingWeapon()?;
            let Some(weapon_ref_val) = weapon_handle.value_reference(memory.view_arc()) else { 
                return Ok(None); 
            };
            
            let weapon_econ = weapon_ref_val.cast::<dyn C_EconEntity>();
            let item_index = weapon_econ.m_AttributeManager()?.m_Item()?.m_iItemDefinitionIndex()?;
            Ok(WeaponId::from_id(item_index as u16))
        })().unwrap_or(None);

        let wid = if let Some(w) = weapon_id {
            w
        } else {
            self.trajectory = None;
            return Ok(());
        };

        if (wid.flags() & WEAPON_FLAG_TYPE_GRENADE) == 0 {
            self.trajectory = None;
            return Ok(());
        }
        
        self.active_type = match wid {
            WeaponId::Smokegrenade => ActiveGrenadeType::Smoke,
            WeaponId::Molotov => ActiveGrenadeType::Molotov,
            WeaponId::Incendiary => ActiveGrenadeType::Molotov,
            WeaponId::Flashbang => ActiveGrenadeType::Flash,
            WeaponId::Decoy => ActiveGrenadeType::Decoy,
            WeaponId::HZgrenade => ActiveGrenadeType::HE,
            _ => {
                let raw_id = wid as u32;
                match raw_id {
                        44 => ActiveGrenadeType::HE,
                        46 | 48 => ActiveGrenadeType::Molotov,
                        _ => ActiveGrenadeType::Unknown
                }
            }
        };

        if self.last_logged_wid != Some(wid) {
            log::info!("Grenade Selected: {:?} (ID: {}), ActiveType: {:?}", wid, wid as u32, self.active_type);
            self.last_logged_wid = Some(wid);
        }

        let left_click = unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 };
        let right_click = unsafe { (GetAsyncKeyState(VK_RBUTTON.0 as i32) as u16 & 0x8000) != 0 };
        if !left_click && !right_click {
             self.trajectory = None;
             return Ok(());
        }

        // Estimate Throw Strength based on buttons
        // Left: 1.0, Right: 0.39, Both: 0.7
        let throw_strength = if left_click && right_click { 0.7 } else if right_click { 0.39 } else { 1.0 };

        let pawn_info = ctx.states.resolve::<StatePawnInfo>(local_pawn_handle)?;
        
        let view_offset = local_pawn.m_vecViewOffset()
            .and_then(|v| Ok(Vector3::new(v.m_vecX()?, v.m_vecY()?, v.m_vecZ()?)))
            .unwrap_or(Vector3::new(0.0, 0.0, 64.0));
            
        let eye_position = pawn_info.position + view_offset;
        let floor_z = pawn_info.position.z;

        let view_angles_vec = local_pawn.m_angEyeAngles()
            .map(|arr| Vector3::new(arr[0], arr[1], arr[2]))
            .unwrap_or(Vector3::new(0.0, 0.0, 0.0));
            
        let player_velocity_vec = local_pawn.m_vecAbsVelocity()
            .map(|arr| Vector3::new(arr[0], arr[1], arr[2]))
            .unwrap_or(Vector3::new(0.0, 0.0, 0.0));

        // Physics Constants from Snippet
        const SV_GRAVITY: f32 = 800.0;
        const TICK_INTERVAL: f32 = 1.0 / 64.0;
        const AIR_DRAG: f32 = 1.0; // Snippet doesn't specify, assuming 1.0
        const ELASTICITY: f32 = 0.45; // Snippet CoR
        const FRICTION: f32 = 0.40;   // Snippet friction

        // Pitch Adjustment
        let mut pitch_deg = view_angles_vec.x;
        if pitch_deg < -89.0 { pitch_deg += 360.0; }
        else if pitch_deg > 89.0 { pitch_deg -= 360.0; }
        
        pitch_deg -= (90.0 - pitch_deg.abs()) * 10.0 / 90.0;

        let pitch = pitch_deg.to_radians();
        let yaw = view_angles_vec.y.to_radians();
        
        let direction = Vector3::new(
            pitch.cos() * yaw.cos(),
            pitch.cos() * yaw.sin(),
            -pitch.sin()
        );

        // Velocity Calculation from Snippet
        // throwVelocity = direction * (throwStrength * 0.7f + 0.3f) * 1115.0f;
        // throwVelocity += playerVelocity * 1.25f;
        let throw_speed = (throw_strength * 0.7 + 0.3) * 1115.0;
        let mut velocity = (direction * throw_speed) + (player_velocity_vec * 1.25);
        
        // Use camera position for start to match game visual
        let mut position = if let Some(cam_pos) = ctx.states.resolve::<ViewController>(()).ok().and_then(|v| v.get_camera_world_position()) {
            cam_pos
        } else {
            eye_position
        };

        // Apply tiny vertical offset as requested
        position.z += 2.0;

        // Check Cache
        let current_state = TrajectoryState {
            position,
            velocity,
            throw_strength,
            weapon_id: wid,
        };

        if let Some(last_state) = &self.last_calc_state {
            if last_state.is_similar(&current_state) && self.trajectory.is_some() {
                // Cache Hit!
                return Ok(());
            }
        }
        
        let mut path = Vec::new();
        let mut accumulated_time = 0.0;
        let detonation_time = self.active_type.get_detonation_time();

        for _step in 0..130 {
            path.push(position);
            
            accumulated_time += TICK_INTERVAL;
            if let Some(det_time) = detonation_time {
                if accumulated_time >= det_time {
                    break;
                }
            }

            // Full Gravity (no 0.25x modifier)
            velocity.z -= SV_GRAVITY * TICK_INTERVAL;
            // velocity *= AIR_DRAG; // Snippet doesn't apply air drag in the loop? 
            // Snippet: currentVel.z -= CS_GRAVITY * CS_TICK_INTERVAL;
            // It does NOT show air drag application. I will comment it out to match snippet.

            let next_position = position + (velocity * TICK_INTERVAL);
            let trace = self.trace_ray(position, next_position);

            if trace.did_hit {
                position = trace.end_pos;
                path.push(position);
                
                // Molotov Impact Logic
                if self.active_type == ActiveGrenadeType::Molotov {
                    // Check if it's a floor (normal.z > 0.7 approx 45 deg)
                    if trace.plane_normal.z > 0.7 {
                        break;
                    }
                }

                // Bounce Physics from Snippet
                // velocityNormal = -velocityNormal * CoR;
                // velocityTangent *= friction;
                
                let normal_velocity = velocity.dot(&trace.plane_normal);
                let velocity_normal = trace.plane_normal * normal_velocity;
                let velocity_tangent = velocity - velocity_normal;
                
                let velocity_normal_after = velocity_normal * -ELASTICITY;
                let velocity_tangent_after = velocity_tangent * FRICTION; // Assuming friction reduces tangent velocity
                
                velocity = velocity_normal_after + velocity_tangent_after;
                
                // Push out of surface slightly? Snippet: currentPos = interpolatedPos + surfaceNormal;
                position += trace.plane_normal * 0.1; 

                if velocity.norm_squared() < 100.0 { break; }
            } else {
                position = next_position;
            }
            if position.z < (floor_z - 1000.0) { break; }
        }

        self.trajectory = Some(path);
        self.last_calc_state = Some(current_state);
        Ok(())
    }

    fn render(
        &mut self,
        states: &StateRegistry,
        ui: &Ui,
        _unicode_text: &UnicodeTextRenderer,
    ) -> Result<()> {
        let view = states.resolve::<ViewController>(())?;
        let draw = ui.get_window_draw_list();

        let Some(trajectory) = &self.trajectory else { return Ok(()); };
        if trajectory.is_empty() { return Ok(()); }

        let landing_pos = trajectory.last().unwrap();
        let (color, radius) = self.active_type.get_visuals();
        let outline_color = [color[0], color[1], color[2], 1.0];
        let fill_color = color; 

        // Render Trajectory Line
        if trajectory.len() > 1 {
            let mut screen_points = Vec::with_capacity(trajectory.len());
            for point in trajectory {
                if let Some(p2d) = view.world_to_screen(point, true) {
                    screen_points.push([p2d.x, p2d.y]);
                }
            }
            if screen_points.len() > 1 {
                 draw.add_polyline(screen_points, outline_color).thickness(2.0).build();
            }
        }

        let segments = 48;
        let mut circle_points = Vec::with_capacity(segments + 1);
        
        for i in 0..=segments {
            let angle = (i as f32 * std::f32::consts::PI * 2.0) / segments as f32;
            let offset = Vector3::new(angle.cos() * radius, angle.sin() * radius, 0.0);
            let point_3d = landing_pos + offset + Vector3::new(0.0, 0.0, 2.0);
            
            if let Some(p2d) = view.world_to_screen(&point_3d, true) {
                circle_points.push([p2d.x, p2d.y]);
            }
        }

        if circle_points.len() > 2 {
            draw.add_polyline(circle_points.clone(), fill_color).filled(true).build();
            draw.add_polyline(circle_points, outline_color).thickness(2.0).build();
            
            if let Some(center_screen) = view.world_to_screen(landing_pos, true) {
                draw.add_circle([center_screen.x, center_screen.y], 3.0, outline_color).filled(true).build();
            }
        }

        Ok(())
    }
}