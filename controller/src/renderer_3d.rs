// src/renderer_3d.rs

use glam::{Mat4, Vec3};
use gltf::Document;
use imgui::TextureId;
use overlay::System as OverlaySystem;
use std::collections::HashMap;
use std::time::Instant;

/// Manages the 3D scene, rendering, and coordinate projection.
pub struct Renderer3D {
    document: Document,
    buffers: Vec<gltf::buffer::Data>,
    scene_texture_id: TextureId,
    start_time: Instant,
}

impl Renderer3D {
    /// Loads the .glb model and creates all necessary GPU resources immediately.
    pub fn new(glb_path: &str, overlay: &mut OverlaySystem) -> anyhow::Result<Self> {
        let (document, buffers, _images) = gltf::import(glb_path)?;
        
        let texture_data = vec![0; 800 * 600 * 4];
        let scene_texture_id = unsafe { overlay.add_texture(&texture_data, 800, 600)? };

        Ok(Self {
            document,
            buffers,
            scene_texture_id,
            start_time: Instant::now(),
        })
    }

    /// Renders a frame of the 3D scene to the internal texture.
    pub fn render_to_texture(&mut self) {
        // This is a mock function. In a real engine, it would render the 3D model.
    }

    /// Returns the TextureId of the rendered 3D scene.
    pub fn get_scene_texture_id(&self) -> TextureId {
        self.scene_texture_id
    }

    /// Calculates the 2D screen positions of each bone for a static preview.
    pub fn get_projected_bones(
        &self,
        window_pos: [f32; 2],
        window_size: [f32; 2],
    ) -> Option<HashMap<String, [f32; 2]>> {
        let scene = self.document.default_scene().or_else(|| self.document.scenes().next())?;
        
        let eye = Vec3::new(0.0, 1.2, 2.5);
        let target = Vec3::new(0.0, 0.9, 0.0);
        let view = Mat4::look_at_rh(eye, target, Vec3::Y);

        let aspect_ratio = if window_size[1] > 0.0 { window_size[0] / window_size[1] } else { 1.0 };
        let projection = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect_ratio, 0.1, 100.0);
        
        // --- MODIFICATION START ---
        // Removed the time-based rotation to make the preview static.
        // The model matrix is now just the identity, meaning no rotation or translation.
        let model_matrix = Mat4::IDENTITY;
        // --- MODIFICATION END ---

        let mvp = projection * view * model_matrix;

        let mut bone_transforms = HashMap::new();
        for node in scene.nodes() {
            Self::calculate_node_transforms(&node, &Mat4::IDENTITY, &mut bone_transforms);
        }

        let mut projected_bones = HashMap::new();
        for (name, global_transform) in bone_transforms {
            let pos_3d = global_transform.w_axis.truncate();
            let clip_space_pos = mvp * pos_3d.extend(1.0);

            if clip_space_pos.w == 0.0 { continue; }
            let ndc = clip_space_pos.truncate() / clip_space_pos.w;
            
            let screen_x = window_pos[0] + (ndc.x + 1.0) / 2.0 * window_size[0];
            let screen_y = window_pos[1] + (1.0 - (ndc.y + 1.0) / 2.0) * window_size[1];
            
            projected_bones.insert(name, [screen_x, screen_y]);
        }
        
        Some(projected_bones)
    }

    fn calculate_node_transforms(
        node: &gltf::Node,
        parent_transform: &Mat4,
        bone_transforms: &mut HashMap<String, Mat4>,
    ) {
        let local_transform = Mat4::from_cols_array_2d(&node.transform().matrix());
        let global_transform = *parent_transform * local_transform;

        if let Some(name) = node.name() {
            bone_transforms.insert(name.to_string(), global_transform);
        }

        for child in node.children() {
            Self::calculate_node_transforms(&child, &global_transform, bone_transforms);
        }
    }
}