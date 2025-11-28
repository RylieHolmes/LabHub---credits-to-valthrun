// controller/src/enhancements/player/info_layout.rs

use imgui::{
    DrawListMut,
    ImColor32,
    TextureId,
};
use crate::settings::{EspTextStyle, EspColor};

#[derive(Clone, Copy, PartialEq)]
pub enum LayoutAlignment {
    Right,
    Bottom,
}

// Stats needed for color calculation
pub struct ColorContext {
    pub health: f32,
    pub distance: f32,
    pub time: f32,
}

pub struct PlayerInfoLayout<'a> {
    ui: &'a imgui::Ui,
    draw: &'a DrawListMut<'a>,

    vmin: nalgebra::Vector2<f32>,
    vmax: nalgebra::Vector2<f32>,

    y_offset: f32,
    
    // Split scaling logic
    scale_text: f32,
    scale_image: f32,

    has_2d_box: bool,
    alignment: LayoutAlignment,
    text_style: EspTextStyle,
}

impl<'a> PlayerInfoLayout<'a> {
    pub fn new(
        ui: &'a imgui::Ui,
        draw: &'a DrawListMut<'a>,
        screen_bounds: mint::Vector2<f32>,
        vmin: nalgebra::Vector2<f32>,
        vmax: nalgebra::Vector2<f32>,
        has_2d_box: bool,
        alignment: LayoutAlignment,
        text_style: EspTextStyle,
    ) -> Self {
        let height_ratio = (vmax.y - vmin.y) / screen_bounds.y;

        // TEXT: Aggressive scaling to stay readable (Min 0.85)
        let text_scale = (height_ratio * 10.0).clamp(0.85, 1.3);
        
        // IMAGE: Natural scaling (Min 3.0), reverting to previous behavior for icons
        let image_scale = (height_ratio * 8.0).clamp(0.5, 1.25);

        // Apply text scale to ImGui context so calc_text_size works
        ui.set_window_font_scale(text_scale);

        Self {
            ui,
            draw,
            vmin,
            vmax,
            y_offset: 0.0,
            scale_text: text_scale,
            scale_image: image_scale,
            has_2d_box,
            alignment,
            text_style,
        }
    }

    // Helper to resolve color based on current layout Y position
    fn resolve_color(&self, color: &EspColor, ctx: &ColorContext) -> [f32; 4] {
        let current_y = match self.alignment {
            LayoutAlignment::Right | LayoutAlignment::Bottom => self.vmin.y + self.y_offset, 
        };
        
        let box_height = self.vmax.y - self.vmin.y;
        let t = if box_height > 0.1 { (current_y - self.vmin.y) / box_height } else { 0.0 };
        
        color.calculate_color(ctx.health, ctx.distance, ctx.time, t)
    }

    pub fn add_line(&mut self, color_setting: &EspColor, ctx: &ColorContext, text: &str) {
        let [text_width, _] = self.ui.calc_text_size(text);
        // Use text_scale for spacing calculations
        let scaled_line_height = self.scale_text * self.ui.text_line_height();

        let (x, y) = match self.alignment {
            LayoutAlignment::Right => {
                let start_x = if self.has_2d_box { self.vmax.x + 4.0 } else { self.vmax.x + 4.0 };
                (start_x, self.vmin.y + self.y_offset)
            },
            LayoutAlignment::Bottom => {
                let center_x = self.vmin.x + (self.vmax.x - self.vmin.x) / 2.0;
                let start_y = self.vmax.y + 4.0; 
                (center_x - text_width / 2.0, start_y + self.y_offset)
            }
        };

        let col = self.resolve_color(color_setting, ctx);

        match self.text_style {
            EspTextStyle::Shadow => {
                let shadow_col = [0.0, 0.0, 0.0, col[3]];
                self.draw.add_text([x + 1.0, y + 1.0], shadow_col, text);
            }
            EspTextStyle::Outline => {
                let outline_col = [0.0, 0.0, 0.0, col[3]];
                self.draw.add_text([x - 1.0, y], outline_col, text);
                self.draw.add_text([x + 1.0, y], outline_col, text);
                self.draw.add_text([x, y - 1.0], outline_col, text);
                self.draw.add_text([x, y + 1.0], outline_col, text);
            }
            EspTextStyle::Neon => {
                let mut glow_col = col;
                glow_col[3] *= 0.3; 
                self.draw.add_text([x - 2.0, y], glow_col, text);
                self.draw.add_text([x + 2.0, y], glow_col, text);
                self.draw.add_text([x, y - 2.0], glow_col, text);
                self.draw.add_text([x, y + 2.0], glow_col, text);
            }
        }

        self.draw.add_text([x, y], col, text);
        self.y_offset += scaled_line_height + 2.0;
    }

    pub fn add_image(&mut self, texture_id: TextureId, color_setting: &EspColor, ctx: &ColorContext, base_height: f32, aspect_ratio: f32) {
        // Use image_scale for images so they shrink nicely at distance
        let height = base_height * self.scale_image;
        let width = height * aspect_ratio;
        
        let col = self.resolve_color(color_setting, ctx);

        let (x, y) = match self.alignment {
            LayoutAlignment::Right => {
                let start_x = if self.has_2d_box { self.vmax.x + 4.0 } else { self.vmax.x + 4.0 };
                (start_x, self.vmin.y + self.y_offset)
            },
            LayoutAlignment::Bottom => {
                let center_x = self.vmin.x + (self.vmax.x - self.vmin.x) / 2.0;
                let start_y = self.vmax.y + 4.0;
                (center_x - width / 2.0, start_y + self.y_offset)
            }
        };

        match self.text_style {
            EspTextStyle::Shadow => {
                 let shadow_col = [0.0, 0.0, 0.0, col[3] * 0.6];
                 self.draw.add_image(texture_id, [x+1.0, y+1.0], [x + width+1.0, y + height+1.0]).col(shadow_col).build();
            },
            EspTextStyle::Outline => {
                let outline_col = [0.0, 0.0, 0.0, col[3]];
                self.draw.add_image(texture_id, [x+1.0, y+1.0], [x + width+1.0, y + height+1.0]).col(outline_col).build();
            },
            EspTextStyle::Neon => {
                let mut glow_col = col;
                glow_col[3] *= 0.3;
                self.draw.add_image(texture_id, [x-2.0, y], [x + width-2.0, y + height]).col(glow_col).build();
                self.draw.add_image(texture_id, [x+2.0, y], [x + width+2.0, y + height]).col(glow_col).build();
                self.draw.add_image(texture_id, [x, y-2.0], [x + width, y + height-2.0]).col(glow_col).build();
                self.draw.add_image(texture_id, [x, y+2.0], [x + width, y + height+2.0]).col(glow_col).build();
            }
        }

        self.draw.add_image(texture_id, [x, y], [x + width, y + height])
            .col(col)
            .build();

        self.y_offset += height + 2.0;
    }
}

impl Drop for PlayerInfoLayout<'_> { fn drop(&mut self) { self.ui.set_window_font_scale(1.0); } }