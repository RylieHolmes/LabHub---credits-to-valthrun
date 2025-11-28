use anyhow::{Context, Result};
use nalgebra::Vector3;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Triangle {
    pub v0: Vector3<f32>,
    pub v1: Vector3<f32>,
    pub v2: Vector3<f32>,
    pub normal: Vector3<f32>,
    pub center: Vector3<f32>, // Pre-calculated for BVH split
}

#[derive(Clone, Debug, Copy)]
struct AABB {
    min: Vector3<f32>,
    max: Vector3<f32>,
}

impl AABB {
    fn new() -> Self {
        Self {
            min: Vector3::new(f32::MAX, f32::MAX, f32::MAX),
            max: Vector3::new(f32::MIN, f32::MIN, f32::MIN),
        }
    }

    fn expand(&mut self, p: &Vector3<f32>) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.min.z = self.min.z.min(p.z);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
        self.max.z = self.max.z.max(p.z);
    }

    fn union(&self, other: &AABB) -> AABB {
        AABB {
            min: Vector3::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
                self.min.z.min(other.min.z),
            ),
            max: Vector3::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
                self.max.z.max(other.max.z),
            ),
        }
    }

    // Slab method for ray-AABB intersection
    #[inline(always)]
    fn intersect(&self, ray_origin: &Vector3<f32>, ray_inv_dir: &Vector3<f32>, t_max: f32) -> bool {
        let tx1 = (self.min.x - ray_origin.x) * ray_inv_dir.x;
        let tx2 = (self.max.x - ray_origin.x) * ray_inv_dir.x;

        let mut tmin = tx1.min(tx2);
        let mut tmax = tx1.max(tx2);

        let ty1 = (self.min.y - ray_origin.y) * ray_inv_dir.y;
        let ty2 = (self.max.y - ray_origin.y) * ray_inv_dir.y;

        tmin = tmin.max(ty1.min(ty2));
        tmax = tmax.min(ty1.max(ty2));

        let tz1 = (self.min.z - ray_origin.z) * ray_inv_dir.z;
        let tz2 = (self.max.z - ray_origin.z) * ray_inv_dir.z;

        tmin = tmin.max(tz1.min(tz2));
        tmax = tmax.min(tz1.max(tz2));

        tmax >= tmin && tmax >= 0.0 && tmin <= t_max
    }
}

// Linear BVH Node
// 32 bytes
#[derive(Clone, Debug, Copy)]
struct LinearNode {
    aabb: AABB,
    // If count > 0, leaf. offset points to first triangle.
    // If count == 0, branch. offset points to right child index. Left child is always current_index + 1.
    offset: u32, 
    count: u16,
    _pad: u16,
}

pub struct MapMesh {
    pub triangles: Vec<Triangle>, // Reordered to match leaf layout
    nodes: Vec<LinearNode>,
}

impl MapMesh {
    // Helper to find the file in common locations
    fn resolve_path(filename: &str) -> Option<PathBuf> {
        // 1. Check absolute path or current working directory
        let p = PathBuf::from(filename);
        if p.exists() { return Some(p); }

        // 2. Check "resources" folder in CWD
        let p_res = PathBuf::from("resources").join(filename);
        if p_res.exists() { return Some(p_res); }

        // 3. Check next to the Executable (Production/Release behavior)
        if let Ok(exe_path) = env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let p_exe = exe_dir.join(filename);
                if p_exe.exists() { return Some(p_exe); }
                
                // Check resources next to exe
                let p_exe_res = exe_dir.join("resources").join(filename);
                if p_exe_res.exists() { return Some(p_exe_res); }
            }
        }

        // 4. Check project root (Development behavior)
        // Often cargo run is executed from workspace root, but file is in crate root
        let p_controller = PathBuf::from("controller").join(filename);
        if p_controller.exists() { return Some(p_controller); }

        None
    }

    pub fn load(filename: &str) -> Result<Self> {
        log::info!("Searching for map physics file: {}", filename);
        
        let path = Self::resolve_path(filename)
            .with_context(|| {
                // Print debug info on failure
                let cwd = env::current_dir().unwrap_or_default();
                format!("Could not find '{}'. Checked CWD: {:?}, Resources, and Exe Dir.", filename, cwd)
            })?;

        log::info!("Found map file at: {:?}", path);

        // Read the file content
        let mut file_bytes = std::fs::read(&path)
            .with_context(|| format!("Failed to read file: {:?}", path))?;

        // Patch the GLB if needed
        match Self::patch_glb_json(&mut file_bytes) {
            Ok(patched) => {
                if patched {
                    log::info!("GLB file patched successfully (in-memory).");
                } else {
                    log::info!("No patches needed.");
                }
            },
            Err(e) => {
                log::warn!("Failed to patch GLB: {}. Attempting to load original.", e);
            }
        }

        // Parse GLB from slice (patched or original)
        // We use from_slice to manually handle buffers and SKIP images (textures)
        // This prevents errors when texture files are missing.
        let gltf = gltf::Gltf::from_slice(&file_bytes)
            .context("Failed to parse GLB structure")?;
            
        let blob = gltf.blob.as_deref();
        let mut buffers = Vec::new();

        for buffer in gltf.document.buffers() {
            let data = match buffer.source() {
                gltf::buffer::Source::Bin => {
                    blob.context("GLB missing binary blob")?.into()
                }
                gltf::buffer::Source::Uri(uri) => {
                    let bin_path = path.parent().unwrap_or(Path::new(".")).join(uri);
                    std::fs::read(&bin_path)
                        .with_context(|| format!("Failed to read external buffer: {:?}", bin_path))?
                }
            };
            buffers.push(gltf::buffer::Data(data));
        }
        
        let document = gltf.document;
        let mut raw_triangles = Vec::new();

        // GLTF is Y-Up, -Z Forward. Source 2 is Z-Up, +X Forward.
        // Transform: Rotate 180 degrees from previous attempt.
        // Previous: (-z, -x, y)
        // New: (z, x, y)
        let to_source2 = |p: Vector3<f32>| -> Vector3<f32> {
            const METERS_TO_INCHES: f32 = 39.3700787;
            Vector3::new(p.z, p.x, p.y) * METERS_TO_INCHES
        };

        // Recursive node traversal to apply transforms
        let mut node_stack = Vec::new();
        for node in document.scenes().next().map(|s| s.nodes()).into_iter().flatten() {
            node_stack.push((node, nalgebra::Matrix4::identity()));
        }

        while let Some((node, parent_transform)) = node_stack.pop() {
            let (t, r, s) = node.transform().decomposed();
            let translation = nalgebra::Vector3::from(t);
            let rotation = nalgebra::UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(r[3], r[0], r[1], r[2]));
            let scale = nalgebra::Vector3::from(s);

            let local_transform = nalgebra::Matrix4::new_translation(&translation)
                * nalgebra::Matrix4::from(rotation.to_rotation_matrix())
                * nalgebra::Matrix4::new_nonuniform_scaling(&scale);

            let world_transform: nalgebra::Matrix4<f32> = parent_transform * local_transform;

            if let Some(mesh) = node.mesh() {
                for primitive in mesh.primitives() {
                    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
                    
                    let Some(positions) = reader.read_positions() else { continue; };
                    let positions: Vec<[f32; 3]> = positions.collect();

                    if let Some(indices) = reader.read_indices() {
                        let indices: Vec<u32> = indices.into_u32().collect();

                        for chunk in indices.chunks(3) {
                            if chunk.len() == 3 {
                                let p0_local = Vector3::from(positions[chunk[0] as usize]);
                                let p1_local = Vector3::from(positions[chunk[1] as usize]);
                                let p2_local = Vector3::from(positions[chunk[2] as usize]);

                                // Apply GLTF Node Transform (to get GLTF World Space)
                                let p0_world_gltf = world_transform.transform_point(&nalgebra::Point3::from(p0_local)).coords;
                                let p1_world_gltf = world_transform.transform_point(&nalgebra::Point3::from(p1_local)).coords;
                                let p2_world_gltf = world_transform.transform_point(&nalgebra::Point3::from(p2_local)).coords;

                                // Convert to Source 2 Coordinates
                                let v0 = to_source2(p0_world_gltf);
                                let v1 = to_source2(p1_world_gltf);
                                let v2 = to_source2(p2_world_gltf);

                                let edge1 = v1 - v0;
                                let edge2 = v2 - v0;
                                let normal = edge1.cross(&edge2).normalize();
                                let center = (v0 + v1 + v2) / 3.0;

                                raw_triangles.push(Triangle { v0, v1, v2, normal, center });
                            }
                        }
                    }
                }
            }

            for child in node.children() {
                node_stack.push((child, world_transform));
            }
        }

        let (triangles, nodes) = if !raw_triangles.is_empty() {
            let mut min_z = f32::MAX;
            let mut max_z = f32::MIN;
            for t in &raw_triangles {
                min_z = min_z.min(t.v0.z).min(t.v1.z).min(t.v2.z);
                max_z = max_z.max(t.v0.z).max(t.v1.z).max(t.v2.z);
            }
            log::info!("Map loaded! {} triangles. Z-Bounds: {:.2} to {:.2}", raw_triangles.len(), min_z, max_z);
            
            log::info!("Building Linear BVH...");
            Self::build_linear_bvh(raw_triangles)
        } else {
            log::warn!("Map loaded from {:?} but contains 0 triangles.", path);
            (Vec::new(), Vec::new())
        };
        
        if !nodes.is_empty() {
            log::info!("Linear BVH built successfully. {} nodes.", nodes.len());
        }

        Ok(Self { triangles, nodes })
    }

    fn build_linear_bvh(mut triangles: Vec<Triangle>) -> (Vec<Triangle>, Vec<LinearNode>) {
        // Temporary recursive node structure
        struct BuildNode {
            aabb: AABB,
            left: Option<Box<BuildNode>>,
            right: Option<Box<BuildNode>>,
            triangle_indices: Vec<usize>, // Indices into ORIGINAL triangles array
        }

        fn recursive_build(triangles: &[Triangle], indices: &mut [usize]) -> BuildNode {
            let mut aabb = AABB::new();
            for &idx in indices.iter() {
                let t = &triangles[idx];
                aabb.expand(&t.v0);
                aabb.expand(&t.v1);
                aabb.expand(&t.v2);
            }

            if indices.len() <= 8 {
                return BuildNode {
                    aabb,
                    left: None,
                    right: None,
                    triangle_indices: indices.to_vec(),
                };
            }

            let extent = aabb.max - aabb.min;
            let axis = if extent.x > extent.y && extent.x > extent.z { 0 }
                       else if extent.y > extent.z { 1 }
                       else { 2 };

            let mid_idx = indices.len() / 2;
            indices.select_nth_unstable_by(mid_idx, |&a, &b| {
                triangles[a].center[axis].partial_cmp(&triangles[b].center[axis]).unwrap_or(std::cmp::Ordering::Equal)
            });

            let (left_indices, right_indices) = indices.split_at_mut(mid_idx);

            BuildNode {
                aabb,
                left: Some(Box::new(recursive_build(triangles, left_indices))),
                right: Some(Box::new(recursive_build(triangles, right_indices))),
                triangle_indices: Vec::new(),
            }
        }

        let mut indices: Vec<usize> = (0..triangles.len()).collect();
        let root = recursive_build(&triangles, &mut indices);

        // Flatten
        let mut flat_nodes = Vec::new();
        let mut ordered_triangles = Vec::with_capacity(triangles.len());

        // Queue for BFS flattening? No, standard is DFS pre-order for "left child is next node".
        // We need to know the index of the right child, which is only known after processing the left subtree.
        
        // Recursive flatten function
        fn flatten(
            node: &BuildNode, 
            nodes: &mut Vec<LinearNode>, 
            ordered_tris: &mut Vec<Triangle>, 
            original_tris: &[Triangle]
        ) -> u32 {
            let node_idx = nodes.len() as u32;
            // Push placeholder
            nodes.push(LinearNode {
                aabb: node.aabb,
                offset: 0,
                count: 0,
                _pad: 0,
            });

            if let (Some(left), Some(right)) = (&node.left, &node.right) {
                // Branch
                let _left_idx = flatten(left, nodes, ordered_tris, original_tris);
                let right_idx = flatten(right, nodes, ordered_tris, original_tris);
                
                // Update current node
                nodes[node_idx as usize].offset = right_idx;
                nodes[node_idx as usize].count = 0;
            } else {
                // Leaf
                let offset = ordered_tris.len() as u32;
                let count = node.triangle_indices.len() as u16;
                
                for &idx in &node.triangle_indices {
                    ordered_tris.push(original_tris[idx].clone());
                }
                
                nodes[node_idx as usize].offset = offset;
                nodes[node_idx as usize].count = count;
            }
            
            node_idx
        }

        flatten(&root, &mut flat_nodes, &mut ordered_triangles, &triangles);
        
        (ordered_triangles, flat_nodes)
    }

    /// Patches the GLB JSON chunk in-memory to ensure it has a "nodes" field.
    fn patch_glb_json(bytes: &mut Vec<u8>) -> Result<bool> {
        use std::io::{Cursor, Read, Write};
        use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};

        let mut cursor = Cursor::new(&bytes);
        
        // 1. Header
        let magic = cursor.read_u32::<LittleEndian>()?;
        if magic != 0x46546C67 { // "glTF"
            return Ok(false);
        }
        let _version = cursor.read_u32::<LittleEndian>()?;
        let _length = cursor.read_u32::<LittleEndian>()?;

        // 2. Find JSON Chunk
        let mut json_chunk_offset = 0;
        let mut json_chunk_length = 0;
        
        while cursor.position() < bytes.len() as u64 {
            let chunk_len = cursor.read_u32::<LittleEndian>()?;
            let chunk_type = cursor.read_u32::<LittleEndian>()?;
            
            if chunk_type == 0x4E4F534A { // "JSON"
                json_chunk_length = chunk_len;
                json_chunk_offset = cursor.position();
                break;
            } else {
                // Skip this chunk
                cursor.set_position(cursor.position() + chunk_len as u64);
            }
        }

        if json_chunk_offset == 0 {
            return Ok(false);
        }

        // 3. Parse JSON
        let start = json_chunk_offset as usize;
        let end = start + json_chunk_length as usize;
        let json_slice = &bytes[start..end];
        
        let mut json_value: serde_json::Value = serde_json::from_slice(json_slice)
            .context("Failed to parse GLB JSON chunk")?;

        // 4. Check and Inject "nodes"
        let mut patched = false;

        if json_value.get("nodes").is_none() {
            if let Some(obj) = json_value.as_object_mut() {
                obj.insert("nodes".to_string(), serde_json::json!([]));
                patched = true;
            }
        }

        // Also check "scenes" for missing "nodes" field
        if let Some(scenes) = json_value.get_mut("scenes").and_then(|v| v.as_array_mut()) {
            for scene in scenes.iter_mut() {
                if scene.get("nodes").is_none() {
                    if let Some(obj) = scene.as_object_mut() {
                        obj.insert("nodes".to_string(), serde_json::json!([]));
                        patched = true;
                    }
                }
            }
        }

        if !patched {
            return Ok(false); 
        }

        // 5. Re-serialize JSON
        let mut new_json_bytes = serde_json::to_vec(&json_value)?;
        
        // 6. Pad JSON data internally to 4-byte boundary
        while new_json_bytes.len() % 4 != 0 {
            new_json_bytes.push(0x20); // Space character
        }
        
        // 7. Reconstruct Binary
        let old_chunk_data_len = json_chunk_length as usize;
        let old_aligned_len = (old_chunk_data_len + 3) & !3;
        let old_chunk_total_size = 8 + old_aligned_len;

        let new_chunk_data_len = new_json_bytes.len(); // Now already aligned
        let new_chunk_total_size = 8 + new_chunk_data_len;
        
        let size_diff = (new_chunk_total_size as i64) - (old_chunk_total_size as i64);
        
        // Construct new vector
        let mut new_file = Vec::with_capacity((bytes.len() as i64 + size_diff) as usize);
        
        // Copy Header
        new_file.write_all(&bytes[0..12])?;
        
        // Update Total Length in Header (offset 8)
        let old_total_len = (&new_file[8..12]).read_u32::<LittleEndian>()?;
        let new_total_len = (old_total_len as i64 + size_diff) as u32;
        let mut len_cursor = Cursor::new(&mut new_file[8..12]);
        len_cursor.write_u32::<LittleEndian>(new_total_len)?;
        
        // Copy data before JSON chunk (if any, usually none for first chunk)
        let chunk_header_offset = json_chunk_offset as usize - 8;
        if chunk_header_offset > 12 {
             new_file.write_all(&bytes[12..chunk_header_offset])?;
        }
        
        // Write New JSON Chunk Header
        new_file.write_u32::<LittleEndian>(new_chunk_data_len as u32)?; // Write ALIGNED length
        new_file.write_u32::<LittleEndian>(0x4E4F534A)?; // "JSON"
        
        // Write New JSON Data (includes internal padding)
        new_file.write_all(&new_json_bytes)?;
        
        // Copy remaining chunks (BIN chunk usually follows)
        let remaining_start = chunk_header_offset + old_chunk_total_size;
        if remaining_start < bytes.len() {
            new_file.write_all(&bytes[remaining_start..])?;
        }
        
        *bytes = new_file;
        Ok(true)
    }

    pub fn check_collision(&self, start: Vector3<f32>, end: Vector3<f32>, radius: f32) -> Option<(f32, Vector3<f32>, Vector3<f32>)> {
        if radius <= 0.001 {
            return self.check_collision_ray(start, end);
        }

        let dir = end - start;
        let len = dir.norm();
        if len < 0.0001 { return None; }
        let dir_norm = dir / len;

        // Compute orthogonal basis
        let up_ref = if dir_norm.z.abs() < 0.99 { Vector3::z() } else { Vector3::x() };
        let right = dir_norm.cross(&up_ref).normalize();
        let up = right.cross(&dir_norm).normalize();

        let offsets = [
            Vector3::zeros(),
            right * radius,
            -right * radius,
            up * radius,
            -up * radius,
        ];

        let mut closest_hit = None;
        let mut min_fraction = 1.0;

        for offset in offsets {
            if let Some((frac, _, normal)) = self.check_collision_ray(start + offset, end + offset) {
                if frac < min_fraction {
                    min_fraction = frac;
                    // Return hit on the central ray
                    let center_hit_pos = start + dir_norm * (len * frac);
                    closest_hit = Some((frac, center_hit_pos, normal));
                }
            }
        }
        closest_hit
    }

    fn check_collision_ray(&self, start: Vector3<f32>, end: Vector3<f32>) -> Option<(f32, Vector3<f32>, Vector3<f32>)> {
        let dir = end - start;
        let len = dir.norm();
        
        if len < 0.0001 { return None; }
        
        let dir_norm = dir / len;
        let inv_dir = Vector3::new(1.0 / dir_norm.x, 1.0 / dir_norm.y, 1.0 / dir_norm.z);

        let mut closest_hit: Option<(f32, Vector3<f32>, Vector3<f32>)> = None; 
        let mut closest_dist = len; 

        if self.nodes.is_empty() { return None; }

        // Stackless traversal using fixed array
        let mut stack = [0u32; 64];
        let mut stack_ptr = 0;
        
        stack[0] = 0; // Push root
        stack_ptr += 1;

        while stack_ptr > 0 {
            stack_ptr -= 1;
            let node_idx = stack[stack_ptr];
            let node = &self.nodes[node_idx as usize];

            if !node.aabb.intersect(&start, &inv_dir, closest_dist) {
                continue;
            }

            if node.count > 0 {
                // Leaf
                let start_idx = node.offset as usize;
                let end_idx = start_idx + node.count as usize;
                
                for i in start_idx..end_idx {
                    let tri = &self.triangles[i];
                    if dir_norm.dot(&tri.normal) > 0.0 { continue; }

                    const EPSILON: f32 = 0.0000001;
                    let edge1 = tri.v1 - tri.v0;
                    let edge2 = tri.v2 - tri.v0;
                    let h = dir_norm.cross(&edge2);
                    let a = edge1.dot(&h);

                    if a > -EPSILON && a < EPSILON { continue; }

                    let f = 1.0 / a;
                    let s = start - tri.v0;
                    let u = f * s.dot(&h);

                    if u < 0.0 || u > 1.0 { continue; }

                    let q = s.cross(&edge1);
                    let v = f * dir_norm.dot(&q);

                    if v < 0.0 || u + v > 1.0 { continue; }

                    let t = f * edge2.dot(&q);

                    if t > EPSILON && t < closest_dist {
                        closest_dist = t;
                        let hit_pos = start + dir_norm * t;
                        let fraction = t / len;
                        closest_hit = Some((fraction, hit_pos, tri.normal));
                    }
                }
            } else {
                // Branch
                // Push children
                // Optimization: Push further child first so we pop closer child first
                // For simplicity, just push both.
                // Left child is always node_idx + 1
                // Right child is node.offset
                
                // Check which child is closer?
                // For now, just push right then left (so left is processed first)
                if stack_ptr < 63 {
                    stack[stack_ptr] = node.offset;
                    stack_ptr += 1;
                    stack[stack_ptr] = node_idx + 1;
                    stack_ptr += 1;
                }
            }
        }

        closest_hit
    }
}