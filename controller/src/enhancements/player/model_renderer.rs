use anyhow::{Context, Result};
use nalgebra::{Matrix4, Vector3, Point3};
use std::path::PathBuf;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use imgui::DrawListMut;
use crate::view::ViewController;

#[derive(Clone, Debug)]
pub struct SkinnedVertex {
    pub position: Vector3<f32>,
    pub normal: Vector3<f32>,
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct SkinnedMesh {
    pub vertices: Vec<SkinnedVertex>,
    pub indices: Vec<u32>,
    pub joint_map: HashMap<usize, String>, // GLTF Joint Index -> Bone Name
    pub inverse_bind_matrices: Vec<Matrix4<f32>>, // Indexed by GLTF Joint Index
    pub joint_parents: HashMap<usize, usize>, // Child Joint Index -> Parent Joint Index
}

#[derive(Clone)]
pub struct CharacterModel {
    pub mesh: Arc<SkinnedMesh>,
    pub missing_bones_logged: Arc<Mutex<HashSet<String>>>,
}

impl CharacterModel {
    pub fn load(filename: &str) -> Result<Self> {
        let path = Self::resolve_path(filename)
            .context(format!("Failed to find character model: {}", filename))?;
            
        log::info!("Loading character model from: {:?}", path);
        
        let file = std::fs::File::open(&path)?;
        let reader = std::io::BufReader::new(file);
        let gltf = gltf::Gltf::from_reader(reader)?;
        
        let buffer_data = gltf::import_buffers(&gltf.document, Some(path.parent().unwrap()), gltf.blob)?;
        
        let mut mesh = SkinnedMesh {
            vertices: Vec::new(),
            indices: Vec::new(),
            joint_map: HashMap::new(),
            inverse_bind_matrices: Vec::new(),
            joint_parents: HashMap::new(),
        };

        // Assume first skin and first mesh
        let skin = gltf.document.skins().next().context("No skin found in GLB")?;
        let reader = skin.reader(|buffer| Some(&buffer_data[buffer.index()]));
        
        // Read Inverse Bind Matrices
        if let Some(ibms) = reader.read_inverse_bind_matrices() {
            mesh.inverse_bind_matrices = ibms.map(|m| Matrix4::from(m)).collect();
        }

        // Map Joints to Names and Build Hierarchy
        let mut node_to_joint_idx = HashMap::new();
        for (joint_idx, joint_node) in skin.joints().enumerate() {
            node_to_joint_idx.insert(joint_node.index(), joint_idx);
            
            if let Some(name) = joint_node.name() {
                // Normalize bone name here
                let normalized_name = Self::normalize_bone_name(name);
                mesh.joint_map.insert(joint_idx, normalized_name);
            }
        }

        for (joint_idx, joint_node) in skin.joints().enumerate() {
            for child in joint_node.children() {
                if let Some(child_joint_idx) = node_to_joint_idx.get(&child.index()) {
                    mesh.joint_parents.insert(*child_joint_idx, joint_idx);
                }
            }
        }

        // Read Mesh Data
        for node in gltf.document.nodes() {
            if let Some(gltf_mesh) = node.mesh() {
                for primitive in gltf_mesh.primitives() {
                    let reader = primitive.reader(|buffer| Some(&buffer_data[buffer.index()]));
                    
                    let positions: Vec<[f32; 3]> = reader.read_positions().context("No positions")?.collect();
                    let normals: Vec<[f32; 3]> = reader.read_normals().map(|iter| iter.collect()).unwrap_or_else(|| vec![[0.0; 3]; positions.len()]);
                    let joints: Vec<[u16; 4]> = reader.read_joints(0).context("No joints")?.into_u16().collect();
                    let weights: Vec<[f32; 4]> = reader.read_weights(0).context("No weights")?.into_f32().collect();
                    
                    let base_index = mesh.vertices.len() as u32;
                    
                    for i in 0..positions.len() {
                        mesh.vertices.push(SkinnedVertex {
                            position: Vector3::from(positions[i]),
                            normal: Vector3::from(normals[i]),
                            joints: joints[i],
                            weights: weights[i],
                        });
                    }
                    
                    if let Some(indices) = reader.read_indices() {
                        mesh.indices.extend(indices.into_u32().map(|i| i + base_index));
                    }
                }
            }
        }

        Ok(Self { 
            mesh: Arc::new(mesh),
            missing_bones_logged: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    fn normalize_bone_name(name: &str) -> String {
        let lower = name.to_lowercase().replace(".", "_").replace(" ", "_");
        let stripped = lower.strip_prefix("mixamorig:").unwrap_or(&lower)
                            .strip_prefix("valvebiped_").unwrap_or(&lower) // Note: dot replaced by underscore
                            .strip_prefix("bip01_").unwrap_or(&lower);
        
        // Extended mapping to handle more rig formats (Rigify, Kenney, etc.)
        match stripped {
            "hips" | "pelvis" | "root" => "pelvis".to_string(),
            
            // Spine
            "spine" | "spine1" | "spine_01" | "spine_1" | "spine01" => "spine_1".to_string(),
            "spine2" | "spine_02" | "spine_2" | "spine02" => "spine_2".to_string(),
            "spine3" | "spine_03" | "spine_3" | "spine03" => "spine_3".to_string(),
            
            // Neck & Head
            "neck" | "neck1" | "neck_01" | "neck_1" | "neck01" => "neck_0".to_string(),
            "head" | "head1" | "head_01" | "head_1" | "head01" => "head_0".to_string(),
            
            // Left Arm
            "leftarm" | "l_upperarm" | "upperarm_l" | 
            "l_arm" | "arm_l" | "left_arm" | 
            "l_upper_arm" | "upper_arm_l" | "left_upper_arm" |
            "shoulder_l" | "l_shoulder" | "arm_upper_l" => "arm_upper_L".to_string(), 
            
            "leftforearm" | "l_forearm" | "forearm_l" |
            "l_fore_arm" | "fore_arm_l" | "left_fore_arm" |
            "l_lowerarm" | "lowerarm_l" | "arm_lower_l" => "arm_lower_L".to_string(),
            
            "lefthand" | "l_hand" | "hand_l" | "left_hand" => "hand_L".to_string(),
            
            // Right Arm
            "rightarm" | "r_upperarm" | "upperarm_r" |
            "r_arm" | "arm_r" | "right_arm" |
            "r_upper_arm" | "upper_arm_r" | "right_upper_arm" |
            "shoulder_r" | "r_shoulder" | "arm_upper_r" => "arm_upper_R".to_string(),
            
            "rightforearm" | "r_forearm" | "forearm_r" |
            "r_fore_arm" | "fore_arm_r" | "right_fore_arm" |
            "r_lowerarm" | "lowerarm_r" | "arm_lower_r" => "arm_lower_R".to_string(),
            
            "righthand" | "r_hand" | "hand_r" | "right_hand" => "hand_R".to_string(),
            
            // Left Leg
            "leftupleg" | "l_thigh" | "thigh_l" |
            "l_up_leg" | "up_leg_l" | "left_up_leg" | "upleg_l" | "l_upleg" |
            "left_thigh" | "l_upper_leg" | "upper_leg_l" | "leg_upper_l" => "leg_upper_L".to_string(),
            
            "leftleg" | "l_calf" | "calf_l" |
            "l_shin" | "shin_l" | "left_shin" | "left_calf" |
            "l_leg" | "leg_l" | "left_leg" | "leg_lower_l" => "leg_lower_L".to_string(), 
            
            "leftfoot" | "l_foot" | "foot_l" | "left_foot" | "ankle_l" => "ankle_L".to_string(),
            
            // Right Leg
            "rightupleg" | "r_thigh" | "thigh_r" |
            "r_up_leg" | "up_leg_r" | "right_up_leg" | "upleg_r" | "r_upleg" |
            "right_thigh" | "r_upper_leg" | "upper_leg_r" | "leg_upper_r" => "leg_upper_R".to_string(),
            
            "rightleg" | "r_calf" | "calf_r" |
            "r_shin" | "shin_r" | "right_shin" | "right_calf" |
            "r_leg" | "leg_r" | "right_leg" | "leg_lower_r" => "leg_lower_R".to_string(),
            
            "rightfoot" | "r_foot" | "foot_r" | "right_foot" | "ankle_r" => "ankle_R".to_string(),

            _ => stripped.to_string(),
        }
    }

    fn resolve_path(filename: &str) -> Option<PathBuf> {
        let p = PathBuf::from(filename);
        if p.exists() { return Some(p); }
        
        let p_res = PathBuf::from("resources").join(filename);
        if p_res.exists() { return Some(p_res); }
        
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let p_exe = dir.join("resources").join(filename);
                if p_exe.exists() { return Some(p_exe); }
            }
        }
        
        // Dev path
        let p_dev = PathBuf::from("controller/resources").join(filename);
        if p_dev.exists() { return Some(p_dev); }

        None
    }

    pub fn render(
        &self,
        draw: &imgui::DrawListMut,
        view: &ViewController,
        bone_transforms: &HashMap<String, Matrix4<f32>>,
        color: [f32; 4],
    ) -> Option<([f32; 2], [f32; 2])> {
        // 0. Pre-calculate Joint Matrices (Optimization: Move matrix mul out of vertex loop)
        // joint_matrices[i] = bone_transform * inverse_bind_matrix
        let mut joint_matrices = vec![Matrix4::identity(); self.mesh.inverse_bind_matrices.len()];
        
        // Fallback transform (Pelvis or Root) to prevent 0,0,0 vertices
        let fallback_transform = bone_transforms.get("pelvis")
            .or_else(|| bone_transforms.get("root"))
            .cloned()
            .unwrap_or_else(Matrix4::identity);

        for (joint_idx, bone_name) in &self.mesh.joint_map {
            if let Some(bone_transform) = bone_transforms.get(bone_name) {
                let ibm = self.mesh.inverse_bind_matrices.get(*joint_idx).cloned().unwrap_or_else(Matrix4::identity);
                joint_matrices[*joint_idx] = bone_transform * ibm;
            } else {
                // Log missing bone (throttled)
                if let Ok(mut logged) = self.missing_bones_logged.try_lock() {
                    if !logged.contains(bone_name) {
                        // log::warn!("Model bone '{}' not found in CS2 bone data.", bone_name);
                        logged.insert(bone_name.clone());
                    }
                }
                let ibm = self.mesh.inverse_bind_matrices.get(*joint_idx).cloned().unwrap_or_else(Matrix4::identity);
                joint_matrices[*joint_idx] = fallback_transform * ibm;
            }
        }

        let mut transformed_vertices = Vec::with_capacity(self.mesh.vertices.len());
        
        // 1. Skinning (Vertex Transformation)
        for v in &self.mesh.vertices {
            let mut skin_matrix = Matrix4::zeros();
            
            // Unroll loop for performance (always 4 weights)
            // v.joints is [u16; 4], v.weights is [f32; 4]
            
            if v.weights[0] > 0.0 { skin_matrix += joint_matrices[v.joints[0] as usize] * v.weights[0]; }
            if v.weights[1] > 0.0 { skin_matrix += joint_matrices[v.joints[1] as usize] * v.weights[1]; }
            if v.weights[2] > 0.0 { skin_matrix += joint_matrices[v.joints[2] as usize] * v.weights[2]; }
            if v.weights[3] > 0.0 { skin_matrix += joint_matrices[v.joints[3] as usize] * v.weights[3]; }

            // Normalize if needed (simple check)
            if skin_matrix.m44 == 0.0 { skin_matrix = Matrix4::identity(); }

            let world_pos = skin_matrix.transform_point(&Point3::from(v.position));
            transformed_vertices.push(world_pos);
        }

        // 2. Triangle Assembly, Backface Culling & Z-Sorting
        struct RenderTri {
            p0: [f32; 2],
            p1: [f32; 2],
            p2: [f32; 2],
            z: f32,
            shade: f32,
        }

        let mut triangles = Vec::with_capacity(self.mesh.indices.len() / 3);
        let cam_pos = view.get_camera_world_position().unwrap_or(Vector3::zeros());
        // Simple directional light from top-left-front
        let light_dir = Vector3::new(0.5, 1.0, 0.5).normalize();

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for chunk in self.mesh.indices.chunks(3) {
            if chunk.len() < 3 { continue; }

            let i0 = chunk[0] as usize;
            let i1 = chunk[1] as usize;
            let i2 = chunk[2] as usize;

            let v0 = transformed_vertices[i0];
            let v1 = transformed_vertices[i1];
            let v2 = transformed_vertices[i2];

            // Backface Culling & Normals
            let edge1 = v1 - v0;
            let edge2 = v2 - v0;
            let normal = edge1.cross(&edge2).normalize();
            
            // Vector from camera to triangle
            let view_dir = v0 - Point3::from(cam_pos);
            
            // If normal points away from camera (dot product > 0), skip
            if normal.dot(&view_dir) >= 0.0 {
                continue;
            }

            // Lighting calculation (Lambertian)
            // Range -1 to 1. Map to 0.3 to 1.0
            let intensity = normal.dot(&light_dir).max(0.0) * 0.7 + 0.3;

            if let (Some(s0), Some(s1), Some(s2)) = (
                view.world_to_screen(&v0.coords, true),
                view.world_to_screen(&v1.coords, true),
                view.world_to_screen(&v2.coords, true),
            ) {
                // Update bounds
                min_x = min_x.min(s0.x).min(s1.x).min(s2.x);
                min_y = min_y.min(s0.y).min(s1.y).min(s2.y);
                max_x = max_x.max(s0.x).max(s1.x).max(s2.x);
                max_y = max_y.max(s0.y).max(s1.y).max(s2.y);

                let dist = ((v0.coords - cam_pos).norm_squared() + (v1.coords - cam_pos).norm_squared() + (v2.coords - cam_pos).norm_squared()) / 3.0;

                triangles.push(RenderTri {
                    p0: [s0.x, s0.y],
                    p1: [s1.x, s1.y],
                    p2: [s2.x, s2.y],
                    z: dist,
                    shade: intensity,
                });
            }
        }

        // Sort back-to-front (Painter's Algorithm)
        triangles.sort_unstable_by(|a, b| b.z.partial_cmp(&a.z).unwrap_or(std::cmp::Ordering::Equal));

        // 3. Draw
        for t in triangles {
            let mut shaded_color = color;
            shaded_color[0] *= t.shade;
            shaded_color[1] *= t.shade;
            shaded_color[2] *= t.shade;
            draw.add_triangle(t.p0, t.p1, t.p2, shaded_color).filled(true).build();
        }

        if min_x < max_x && min_y < max_y {
            Some(([min_x, min_y], [max_x, max_y]))
        } else {
            None
        }
    }
}
