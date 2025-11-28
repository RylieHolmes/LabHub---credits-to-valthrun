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
        };

        // Assume first skin and first mesh
        let skin = gltf.document.skins().next().context("No skin found in GLB")?;
        let reader = skin.reader(|buffer| Some(&buffer_data[buffer.index()]));
        
        // Read Inverse Bind Matrices
        if let Some(ibms) = reader.read_inverse_bind_matrices() {
            mesh.inverse_bind_matrices = ibms.map(|m| Matrix4::from(m)).collect();
        }

        // Map Joints to Names
        for (joint_idx, joint_node) in skin.joints().enumerate() {
            if let Some(name) = joint_node.name() {
                // Normalize bone name here
                let normalized_name = Self::normalize_bone_name(name);
                mesh.joint_map.insert(joint_idx, normalized_name);
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
        let lower = name.to_lowercase();
        let stripped = lower.strip_prefix("mixamorig:").unwrap_or(&lower)
                            .strip_prefix("valvebiped.").unwrap_or(&lower)
                            .strip_prefix("bip01_").unwrap_or(&lower);
        
        // Basic mapping
        match stripped {
            "hips" | "pelvis" => "pelvis".to_string(),
            "spine" | "spine1" | "spine_01" => "spine_1".to_string(),
            "spine2" | "spine_02" => "spine_2".to_string(),
            "spine3" | "spine_03" => "spine_3".to_string(),
            "neck" | "neck1" => "neck_0".to_string(),
            "head" => "head_0".to_string(),
            
            "leftarm" | "l_upperarm" | "upperarm_l" => "arm_upper_L".to_string(),
            "leftforearm" | "l_forearm" | "forearm_l" => "arm_lower_L".to_string(),
            "lefthand" | "l_hand" | "hand_l" => "hand_L".to_string(),
            
            "rightarm" | "r_upperarm" | "upperarm_r" => "arm_upper_R".to_string(),
            "rightforearm" | "r_forearm" | "forearm_r" => "arm_lower_R".to_string(),
            "righthand" | "r_hand" | "hand_r" => "hand_R".to_string(),
            
            "leftupleg" | "l_thigh" | "thigh_l" => "leg_upper_L".to_string(),
            "leftleg" | "l_calf" | "calf_l" => "leg_lower_L".to_string(),
            "leftfoot" | "l_foot" | "foot_l" => "ankle_L".to_string(),
            
            "rightupleg" | "r_thigh" | "thigh_r" => "leg_upper_R".to_string(),
            "rightleg" | "r_calf" | "calf_r" => "leg_lower_R".to_string(),
            "rightfoot" | "r_foot" | "foot_r" => "ankle_R".to_string(),

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
    ) {
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
        }

        let mut triangles = Vec::with_capacity(self.mesh.indices.len() / 3);
        let cam_pos = view.get_camera_world_position().unwrap_or(Vector3::zeros());

        for chunk in self.mesh.indices.chunks(3) {
            if chunk.len() < 3 { continue; }

            let i0 = chunk[0] as usize;
            let i1 = chunk[1] as usize;
            let i2 = chunk[2] as usize;

            let v0 = transformed_vertices[i0];
            let v1 = transformed_vertices[i1];
            let v2 = transformed_vertices[i2];

            // Backface Culling
            // Normal of the triangle
            let edge1 = v1 - v0;
            let edge2 = v2 - v0;
            let normal = edge1.cross(&edge2);
            
            // Vector from camera to triangle
            let view_dir = v0 - Point3::from(cam_pos);
            
            // If normal points away from camera (dot product > 0), skip
            // Note: Winding order matters. Assuming CCW.
            if normal.dot(&view_dir) >= 0.0 {
                continue;
            }

            if let (Some(s0), Some(s1), Some(s2)) = (
                view.world_to_screen(&v0.coords, true),
                view.world_to_screen(&v1.coords, true),
                view.world_to_screen(&v2.coords, true),
            ) {
                // Screen-Space Area Culling
                // Area = 0.5 * |x1(y2 - y3) + x2(y3 - y1) + x3(y1 - y2)|
                // let area = 0.5 * (s0.x * (s1.y - s2.y) + s1.x * (s2.y - s0.y) + s2.x * (s0.y - s1.y)).abs();
                // if area < 1.0 { // Cull triangles smaller than 1.0 pixel
                //    continue;
                // }

                let dist = ((v0.coords - cam_pos).norm_squared() + (v1.coords - cam_pos).norm_squared() + (v2.coords - cam_pos).norm_squared()) / 3.0;

                triangles.push(RenderTri {
                    p0: [s0.x, s0.y],
                    p1: [s1.x, s1.y],
                    p2: [s2.x, s2.y],
                    z: dist,
                });
            }
        }

        // Sort back-to-front (Painter's Algorithm)
        // Use norm_squared for distance to avoid sqrt, so larger is further
        triangles.sort_unstable_by(|a, b| b.z.partial_cmp(&a.z).unwrap_or(std::cmp::Ordering::Equal));

        // 3. Draw
        for t in triangles {
            draw.add_triangle(t.p0, t.p1, t.p2, color).filled(true).build();
        }
    }
}
