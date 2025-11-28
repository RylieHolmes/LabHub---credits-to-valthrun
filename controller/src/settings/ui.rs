// controller/src/settings/ui.rs

use std::{
    collections::{
        btree_map::Entry,
        BTreeMap,
        HashMap,
    },
    num::NonZeroIsize,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use cs2::StateBuildInfo;
use font_awesome;
use imgui::{
    Condition,
    Image,
    StyleColor,
    StyleVar,
    WindowFlags,
    TextureId,
};

use overlay::UnicodeTextRenderer;
use rfd::FileDialog;

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, Win32WindowHandle, WindowHandle as RawWindowHandleType, WindowsDisplayHandle,
};
use windows::{
    core::s,
    Win32::{Foundation::HWND, UI::WindowsAndMessaging::FindWindowA},
};

use super::{
    config::AppSettings,
    config_manager,
    esp::{
        Color,
        EspColor,
        EspColorType,
        EspConfig,
        EspSelector,
        EspBoxType,
        EspHeadDot,
        EspHealthBar,
        EspPlayerSettings,
        EspTracePosition,
    },
    config::KeyToggleMode,
};
use crate::{
    utils::{
        imgui::ImguiUiEx,
        ImGuiKey,
        ImguiComboEnum,
    },
    Application,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ActiveTab {
    Visuals,
    TriggerBot,
    LegitAim,
    Crosshair,
    World,
    Overlay,
    Hotkeys,
    Config,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PlayerTargetMode {
    Friendly,
    Enemy,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FlagType {
    Kit,
    Scoped,
    Flashed,
    BombCarrier,
}

#[derive(Debug, Clone, Copy)]
struct WindowHandle(HWND);

impl HasWindowHandle for WindowHandle {
    fn window_handle(&self) -> Result<RawWindowHandleType, HandleError> {
        let hwnd = NonZeroIsize::new(self.0.0).ok_or(HandleError::Unavailable)?;
        let handle = Win32WindowHandle::new(hwnd);
        Ok(unsafe { RawWindowHandleType::borrow_raw(RawWindowHandle::Win32(handle)) })
    }
}

impl HasDisplayHandle for WindowHandle {
    fn display_handle(&self) -> Result<DisplayHandle, HandleError> {
        Ok(unsafe { DisplayHandle::borrow_raw(RawDisplayHandle::Windows(WindowsDisplayHandle::new())) })
    }
}

#[derive(Clone, Copy)]
struct WidgetAnimationState {
    progress: f32,
}

// --- PREVIEW CONFIGURATION STRUCT ---
struct PreviewLayoutConfig {
    global_scale_pad: f32,
    
    // Offsets
    character_offset: [f32; 2],
    skeleton_offset: [f32; 2],
    head_offset: [f32; 2],
    weapon_offset: [f32; 2],
    distance_offset: [f32; 2],
    ammo_offset: [f32; 2],
    health_bar_padding: f32,
    name_padding: [f32; 2],
    
    // Individual Scales
    character_scale: f32,
    skeleton_scale: f32,
    head_scale: f32,
    weapon_scale: f32,
    distance_scale: f32,
    ammo_scale: f32,
    name_scale: f32,
    health_bar_scale: f32,
}

impl Default for PreviewLayoutConfig {
    fn default() -> Self {
        Self {
            global_scale_pad: 0.55,
            
            // Offsets
            character_offset: [0.0, 0.0],
            skeleton_offset: [-38.0, 0.0],
            head_offset: [-63.0, -456.0],
            weapon_offset: [0.0, 656.0],
            distance_offset: [0.0, 830.0],
            ammo_offset: [5.0, 752.0],
            health_bar_padding: -25.0,
            name_padding: [-19.0, -36.0],
            
            // Scales
            character_scale: 2.0,
            skeleton_scale: 0.75,
            head_scale: 0.6, 
            weapon_scale: 3.25,
            distance_scale: 3.0,
            ammo_scale: 2.65,
            name_scale: 2.95,
            health_bar_scale: 2.0,
        }
    }
}

pub struct SettingsUI {
    discord_link_copied: Option<Instant>,
    active_tab: ActiveTab,
    tab_offsets: BTreeMap<ActiveTab, f32>,
    content_y_offset: f32,
    esp_player_target_mode: PlayerTargetMode,
    config_list: Vec<String>,
    selected_config_index: Option<usize>,
    new_config_name: String,
    needs_config_refresh: bool,
    
    // Animations
    checkbox_animations: HashMap<String, WidgetAnimationState>,
    cog_animations: HashMap<String, WidgetAnimationState>,

    // Dropdown Animation States
    dropdown_animations: HashMap<String, WidgetAnimationState>,
    dropdown_content_heights: HashMap<String, f32>,
    open_dropdowns: Vec<String>,

    active_flag_setting: Option<FlagType>,
    ui_alpha: f32,
    is_first_render: bool,
    start_time: Instant,
    preview_layout: PreviewLayoutConfig,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

impl SettingsUI {
    pub fn new() -> Self {
        Self {
            discord_link_copied: None,
            active_tab: ActiveTab::Visuals,
            tab_offsets: BTreeMap::new(),
            content_y_offset: 0.0,
            esp_player_target_mode: PlayerTargetMode::Enemy,
            config_list: Vec::new(),
            selected_config_index: None,
            new_config_name: String::with_capacity(32),
            needs_config_refresh: true,
            checkbox_animations: HashMap::new(),
            cog_animations: HashMap::new(),
            dropdown_animations: HashMap::new(),
            dropdown_content_heights: HashMap::new(),
            open_dropdowns: Vec::new(),
            active_flag_setting: None,
            ui_alpha: 0.0,
            is_first_render: true,
            start_time: Instant::now(),
            preview_layout: PreviewLayoutConfig::default(),
        }
    }

    fn render_sidebar_button(
        &mut self,
        ui: &imgui::Ui,
        label: &str,
        icon: char,
        tab: ActiveTab,
        _sidebar_width: f32,
    ) {
        let is_active = self.active_tab == tab;
        let animation_speed = 8.0;
        let moved_offset = 10.0;
        let button_width = 130.0;
        
        let current_offset = self.tab_offsets.entry(tab).or_insert(0.0);
        
        let original_cursor_pos = ui.cursor_pos();
        
        let available_width = ui.content_region_avail()[0];
        let center_start_pos = (available_width - button_width) / 2.0;

        let final_x_pos = center_start_pos - (moved_offset / 2.0) + *current_offset;
        ui.set_cursor_pos([original_cursor_pos[0] + final_x_pos, original_cursor_pos[1]]);

        let text = format!("{} {}", icon, label);
        let style = if is_active {
            Some(ui.push_style_color(StyleColor::Button, [0.15, 0.45, 0.75, 1.0]))
        } else {
            None
        };

        let clicked = ui.button_with_size(text, [button_width, 30.0]);
        let is_hovered = ui.is_item_hovered() && self.ui_alpha > 0.01;

        if let Some(s) = style {
            s.pop();
        }

        if clicked {
            self.active_tab = tab;
        }

        let target_offset = if is_active || is_hovered {
            moved_offset
        } else {
            0.0
        };

        *current_offset += (target_offset - *current_offset) * animation_speed * ui.io().delta_time;
    }

    pub fn render(
        &mut self,
        app: &Application,
        ui: &imgui::Ui,
        _unicode_text: &UnicodeTextRenderer,
    ) {
        if self.is_first_render {
            let total_elapsed = self.start_time.elapsed();
            let delay = Duration::from_secs(5);

            if total_elapsed < delay {
                return;
            }

            const INTRO_TOTAL_DURATION: Duration = Duration::from_millis(3000);
            let elapsed = total_elapsed - delay;

            if elapsed >= INTRO_TOTAL_DURATION {
                self.ui_alpha = 1.0;
                self.is_first_render = false;
            } else {
                self.render_typewriter_intro(ui, app, elapsed);
                return;
            }
        } else {
            let target_alpha = if app.settings_visible { 1.0 } else { 0.0 };
            let animation_speed = 7.5;
            self.ui_alpha += (target_alpha - self.ui_alpha) * animation_speed * ui.io().delta_time;
            self.ui_alpha = self.ui_alpha.clamp(0.0, 1.0);
        }

        if self.ui_alpha < 0.001 {
            return;
        }
        
        let mut settings = app.settings_mut();
        let Some(title_font_id) = app.fonts.title.font_id() else { return };
        let Some(content_font_id) = app.fonts.labh.font_id() else { return };

        let _title_font_guard = ui.push_font(title_font_id);
        let _border = ui.push_style_var(StyleVar::WindowBorderSize(0.0));
        let _alpha_guard = ui.push_style_var(StyleVar::Alpha(self.ui_alpha));
        let _bg_color = ui.push_style_color(StyleColor::WindowBg, [0.02, 0.02, 0.03, 1.0]);
        
        const WINDOW_SIZE: [f32; 2] = [1024.0, 768.0];

        let display_size = ui.io().display_size;
        let window_pos = [
            (display_size[0] - WINDOW_SIZE[0]) * 0.5,
            (display_size[1] - WINDOW_SIZE[1]) * 0.5,
        ];

        let mut flags = WindowFlags::NO_DECORATION;
        if !app.settings_visible || self.is_first_render {
            flags |= WindowFlags::NO_INPUTS;
        }

        ui.window(format!("LABHub v{}", VERSION))
            .size(WINDOW_SIZE, Condition::Always)
            .position(window_pos, Condition::Always)
            .flags(flags)
            .build(|| {
                let _content_font_guard = ui.push_font(content_font_id);
                let _style = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));

                {
                    let title_bar_height = 35.0;
                    let _title_bg = ui.push_style_color(StyleColor::ChildBg, [0.02, 0.02, 0.03, 1.0]);

                    ui.child_window("TitleBar")
                        .size([WINDOW_SIZE[0], title_bar_height])
                        .build(|| {
                            let _font = ui.push_font(title_font_id);
                            
                            ui.set_cursor_pos([15.0, 8.0]);
                            let logo_letters = [
                                ("L", [0.8, 0.8, 0.8, 1.0]),
                                ("A", [0.7, 0.7, 0.7, 1.0]),
                                ("B", [0.6, 0.6, 0.6, 1.0]),
                                ("H", [0.5, 0.5, 0.5, 1.0]),
                                ("u", [0.4, 0.4, 0.4, 1.0]),
                                ("b", [0.3, 0.3, 0.3, 1.0]),
                            ];
                            
                            for (i, (text, color)) in logo_letters.iter().enumerate() {
                                let mut color = *color;
                                color[3] *= self.ui_alpha;
                                ui.text_colored(color, text);
                                if i < logo_letters.len() - 1 {
                                    ui.same_line();
                                }
                            }
                        });
                }

                let sidebar_width = 180.0;
                let _sidebar_bg = ui.push_style_color(StyleColor::ChildBg, [0.02, 0.02, 0.03, 1.0]);
                
                let previous_tab = self.active_tab;

                ui.child_window("Sidebar")
                    .size([sidebar_width, 0.0])
                    .build(|| {
                        let _padding = ui.push_style_var(StyleVar::WindowPadding([15.0, 15.0]));
                        ui.dummy([0.0, 10.0]);

                        let render_sidebar_label = |ui: &imgui::Ui, label: &str| {
                            ui.dummy([0.0, 8.0]);
                            let text_width = ui.calc_text_size(label)[0];
                            let available_width = ui.content_region_avail()[0];
                            ui.set_cursor_pos_x((available_width - text_width) * 0.5);
                            ui.text_disabled(label);
                            ui.dummy([0.0, 8.0]);
                        };

                        render_sidebar_label(ui, "- visuals -");
                        self.render_sidebar_button(ui, "Player", font_awesome::EYE, ActiveTab::Visuals, sidebar_width);
                        self.render_sidebar_button(ui, "World", font_awesome::GLOBE, ActiveTab::World, sidebar_width);
                        self.render_sidebar_button(ui, "Overlay", font_awesome::DESKTOP, ActiveTab::Overlay, sidebar_width);
                        
                        render_sidebar_label(ui, "- aim -");
                        self.render_sidebar_button(ui, "Trigger Bot", font_awesome::BULLSEYE, ActiveTab::TriggerBot, sidebar_width);
                        self.render_sidebar_button(ui, "Legit Aim", font_awesome::CROSSHAIRS, ActiveTab::LegitAim, sidebar_width);
                        self.render_sidebar_button(ui, "Crosshair", font_awesome::CROSSHAIRS, ActiveTab::Crosshair, sidebar_width);
                        
                        render_sidebar_label(ui, "- misc -");
                        self.render_sidebar_button(ui, "Hotkeys", font_awesome::KEYBOARD, ActiveTab::Hotkeys, sidebar_width);
                        self.render_sidebar_button(ui, "Config", font_awesome::SAVE, ActiveTab::Config, sidebar_width);
                        self.render_sidebar_button(ui, "Info", font_awesome::INFO_CIRCLE, ActiveTab::Info, sidebar_width);
                    });

                ui.same_line_with_spacing(0.0, 0.0);
                
                {
                    let p = ui.cursor_screen_pos();
                    let draw_list = ui.get_window_draw_list();
                    let separator_color = [0.2, 0.2, 0.2, 0.5 * self.ui_alpha];
                    let height = ui.content_region_avail()[1];
                    draw_list.add_line(
                        p,
                        [p[0], p[1] + height],
                        separator_color
                    ).build();
                    ui.dummy([1.0, height]); 
                }
                
                ui.same_line_with_spacing(0.0, 4.0);

                let initial_y_offset = 40.0;
                if self.active_tab != previous_tab {
                    self.content_y_offset = initial_y_offset;
                }
                let animation_speed = 10.0;
                self.content_y_offset += (0.0 - self.content_y_offset) * animation_speed * ui.io().delta_time;
                
                let original_cursor_pos = ui.cursor_pos();
                ui.set_cursor_pos([original_cursor_pos[0], original_cursor_pos[1] + self.content_y_offset]);
                
                let slide_up_alpha = 1.0 - (self.content_y_offset / initial_y_offset).clamp(0.0, 1.0);
                let _content_bg = ui.push_style_color(StyleColor::ChildBg, [0.0, 0.0, 0.0, 0.0]);

                ui.child_window("Content")
                    .build(|| {
                        let _padding = ui.push_style_var(StyleVar::WindowPadding([15.0, 15.0]));
                        match self.active_tab {
                            ActiveTab::Visuals => {
                                self.render_esp_settings(app, &mut *settings, ui);
                            }
                            ActiveTab::TriggerBot => {
                                ui.set_next_item_width(150.0);
                                ui.combo_enum(
                                    "Trigger Bot",
                                    &[
                                        (KeyToggleMode::Off, "Off"),
                                        (KeyToggleMode::Trigger, "Hold"),
                                        (KeyToggleMode::TriggerInverted, "Hold Inverted"),
                                        (KeyToggleMode::Toggle, "Toggle"),
                                        (KeyToggleMode::AlwaysOn, "On"),
                                    ],
                                    &mut settings.trigger_bot_mode
                                );
            
                                if !matches!(settings.trigger_bot_mode, KeyToggleMode::Off | KeyToggleMode::AlwaysOn) {
                                    ui.button_key_optional(
                                        "Trigger bot key",
                                        &mut settings.key_trigger_bot,
                                        [150.0, 0.0]
                                    );
                                }
                                
                                if !matches!(settings.trigger_bot_mode, KeyToggleMode::Off) {
                                    let mut values_updated = false;
                                    let slider_width = (ui.current_column_width() / 2.0 - 80.0).min(300.0).max(50.0);
                                    let slider_width_1 = (ui.current_column_width() / 2.0 - 20.0).min(300.0).max(50.0);
            
                                    ui.text("Trigger delay min: ");
                                    ui.same_line();
                                    ui.set_next_item_width(slider_width);
                                    values_updated |= ui.slider_config("##delay_min", 0, 300).display_format("%dms").build(&mut settings.trigger_bot_delay_min);
                                    ui.same_line();
            
                                    ui.text(" max: ");
                                    ui.same_line();
                                    ui.set_next_item_width(slider_width);
                                    values_updated |= ui.slider_config("##delay_max", 0, 300).display_format("%dms").build(&mut settings.trigger_bot_delay_max);
            
                                    ui.text("Shoot duration: ");
                                    ui.same_line();
                                    ui.set_next_item_width(slider_width_1);
                                    values_updated |= ui.slider_config("##shoot_duration", 0, 1000).display_format("%dms").build(&mut settings.trigger_bot_shot_duration);
            
                                    if values_updated {
                                        let delay_min = settings.trigger_bot_delay_min.min(settings.trigger_bot_delay_max);
                                        let delay_max = settings.trigger_bot_delay_min.max(settings.trigger_bot_delay_max);
            
                                        settings.trigger_bot_delay_min = delay_min;
                                        settings.trigger_bot_delay_max = delay_max;
                                    }
            
                                    self.animated_checkbox(ui, "Retest trigger target after delay", &mut settings.trigger_bot_check_target_after_delay);
                                    self.animated_checkbox(ui, "Team Check", &mut settings.trigger_bot_team_check);
                                }
                                ui.separator();
                                self.animated_checkbox(ui, "Simple Recoil Helper", &mut settings.aim_assist_recoil);
                            }
                            ActiveTab::LegitAim => {
                                ui.text("Legit Aim Settings");
                                ui.separator();
                                self.animated_checkbox(ui, "Enabled", &mut settings.legit_aim_enabled);

                                if settings.legit_aim_enabled {
                                    ui.indent();
                                    
                                    ui.button_key_optional(
                                        "Activation Key",
                                        &mut settings.legit_aim_key,
                                        [150.0, 0.0]
                                    );

                                    ui.slider_config("FOV", 1.0, 180.0).display_format("%.1f px").build(&mut settings.legit_aim_fov);
                                    ui.slider_config("Smoothing", 1.0, 50.0).display_format("%.1f").build(&mut settings.legit_aim_smooth);
                                    
                                    let mut current_bone = settings.legit_aim_bone.clone();
                                    let bones = ["head_0", "neck_0", "spine_1", "spine_2", "pelvis"];
                                    let mut selected_bone_idx = bones.iter().position(|&b| b == current_bone).unwrap_or(0);
                                    
                                    ui.set_next_item_width(150.0);
                                    if ui.combo("Target Bone", &mut selected_bone_idx, &bones, |b| b.to_string().into()) {
                                        settings.legit_aim_bone = bones[selected_bone_idx].to_string();
                                    }

                                    ui.unindent();
                                }
                            }
                            ActiveTab::Crosshair => {
                                ui.text("Crosshair Settings");
                                ui.separator();
                                self.animated_checkbox(ui, "Sniper Crosshair", &mut settings.sniper_crosshair);

                                let _disabled = ui.begin_disabled(!settings.sniper_crosshair);
                                ui.indent();
                                let style = &mut settings.sniper_crosshair_settings;
                                
                                ui.slider_config("Size", 0.0, 20.0).build(&mut style.size);
                                ui.slider_config("Thickness", 0.1, 10.0).build(&mut style.thickness);
                                ui.slider_config("Gap", -20.0, 20.0).build(&mut style.gap);
                                ui.slider_config("Outline Thickness", 0.1, 5.0).build(&mut style.outline_thickness);
                                
                                self.animated_checkbox(ui, "Dot", &mut style.dot);
                                ui.same_line();
                                self.animated_checkbox(ui, "Outline", &mut style.outline);
                                
                                let mut color_f32 = [
                                    style.color[0] as f32 / 255.0,
                                    style.color[1] as f32 / 255.0,
                                    style.color[2] as f32 / 255.0,
                                    style.color[3] as f32 / 255.0,
                                ];
                                
                                if ui.color_edit4_config("Color", &mut color_f32).alpha(true).build() {
                                    style.color = [
                                        (color_f32[0] * 255.0) as u8,
                                        (color_f32[1] * 255.0) as u8,
                                        (color_f32[2] * 255.0) as u8,
                                        (color_f32[3] * 255.0) as u8,
                                    ];
                                }
                                ui.unindent();
                            }
                            ActiveTab::World => {
                                ui.text("World");
                                ui.separator();
                                self.animated_checkbox(ui, "Bomb Timer", &mut settings.bomb_timer);
                                self.animated_checkbox(ui, "Bomb Site Label", &mut settings.bomb_label);
                                
                                self.animated_checkbox(ui, "Grenade Trajectory", &mut settings.grenade_trajectory.enabled);
                            }
                            ActiveTab::Overlay => {
                                ui.text("Overlay");
                                ui.separator();
                                self.animated_checkbox(ui, "Spectators List", &mut settings.spectators_list);
                                self.animated_checkbox(ui, "Watermark", &mut settings.labh_watermark);
            
                                if self.animated_checkbox(
                                    ui,
                                    "Hide overlay from screen capture",
                                    &mut settings.hide_overlay_from_screen_capture
                                ) {
                                    app.settings_screen_capture_changed.store(true, Ordering::Relaxed);
                                }
            
                                if self.animated_checkbox(
                                    ui,
                                    "Show render debug overlay",
                                    &mut settings.render_debug_window
                                ) {
                                    app.settings_render_debug_window_changed.store(true, Ordering::Relaxed);
                                }
                            }
                            ActiveTab::Hotkeys => {
                                ui.button_key_ignore_mouse_left(
                                    "Toggle Settings",
                                    &mut settings.key_settings,
                                    [150.0, 0.0]
                                );
            
                                {
                                    let _enabled = ui.begin_enabled(matches!(
                                        settings.esp_mode,
                                        KeyToggleMode::Toggle | KeyToggleMode::Trigger
                                    ));
                                    ui.button_key_optional(
                                        "ESP Toggle/Hold",
                                        &mut settings.esp_toggle,
                                        [150.0, 0.0]
                                    );
                                }
                            }
                            ActiveTab::Config => {
                                if self.needs_config_refresh {
                                    match config_manager::list_configs() {
                                        Ok(configs) => self.config_list = configs,
                                        Err(e) => log::error!("Failed to list configs: {}", e),
                                    }
                                    self.selected_config_index = None;
                                    self.needs_config_refresh = false;
                                }

                                ui.text("Configuration Management");
                                ui.separator();

                                let list_height = ui.content_region_avail()[1] - ui.frame_height_with_spacing() * 2.5;
                                
                                ui.child_window("ConfigList").border(true).size([0.0, list_height]).build(|| {
                                    for (i, name) in self.config_list.iter().enumerate() {
                                        let is_selected = self.selected_config_index == Some(i);
                                        if ui.selectable_config(name).selected(is_selected).build() {
                                            self.selected_config_index = Some(i);
                                            self.new_config_name = name.clone();
                                        }
                                    }
                                });
                                ui.separator();

                                let button_width = 70.0;
                                let spacing = unsafe { ui.style().item_spacing[0] - 2.0 };
                                let total_button_width = (button_width * 4.0) + (spacing * 3.0);
                                
                                ui.set_next_item_width(-total_button_width - spacing);
                                ui.input_text("##ConfigName", &mut self.new_config_name).hint("Enter config name...").build();
                                
                                let button_size = [button_width, 0.0];
                                
                                ui.same_line_with_spacing(0.0, spacing);
                                let load_disabled = self.selected_config_index.is_none();
                                let _disabled_load = ui.begin_disabled(load_disabled);
                                if ui.button_with_size("Load", button_size) {
                                    if let Some(index) = self.selected_config_index {
                                        let config_name = &self.config_list[index];
                                        match config_manager::load_config(config_name) {
                                            Ok(new_settings) => *settings = new_settings,
                                            Err(e) => log::error!("Failed to load config '{}': {}", config_name, e),
                                        }
                                    }
                                }
                                _disabled_load.end();

                                ui.same_line_with_spacing(0.0, spacing);
                                let save_disabled = self.new_config_name.trim().is_empty();
                                let _disabled_save = ui.begin_disabled(save_disabled);
                                if ui.button_with_size("Save", button_size) {
                                     let name_to_save = self.new_config_name.trim();
                                     match config_manager::save_config(name_to_save, &settings) {
                                         Ok(_) => self.needs_config_refresh = true,
                                         Err(e) => log::error!("Failed to save config '{}': {}", name_to_save, e),
                                     }
                                }
                                _disabled_save.end();
                                
                                ui.same_line_with_spacing(0.0, spacing);
                                if ui.button_with_size("Import", button_size) {
                                    let hwnd = unsafe { FindWindowA(None, s!("CS2 Overlay")) };
                                    let mut dialog = FileDialog::new().add_filter("YAML Config", &["yaml", "yml"]);
                                    if hwnd.0 != 0 { dialog = dialog.set_parent(&WindowHandle(hwnd)); }
                                    if let Some(path) = dialog.pick_file() {
                                        match config_manager::import_config(&path) {
                                            Ok(_) => self.needs_config_refresh = true,
                                            Err(e) => log::error!("Failed to import config: {}", e),
                                        }
                                    }
                                }

                                ui.same_line_with_spacing(0.0, spacing);
                                let mut delete_disabled = self.selected_config_index.is_none();
                                if let Some(index) = self.selected_config_index {
                                    if &self.config_list[index] == "default" {
                                        delete_disabled = true;
                                    }
                                }
                                let _disabled_delete = ui.begin_disabled(delete_disabled);
                                let _red_button = ui.push_style_color(StyleColor::Button, [0.6, 0.2, 0.2, 1.0]);
                                if ui.button_with_size("Delete", button_size) {
                                    if let Some(index) = self.selected_config_index {
                                        let config_name = &self.config_list[index];
                                        match config_manager::delete_config(config_name) {
                                            Ok(_) => self.needs_config_refresh = true,
                                            Err(e) => log::error!("Failed to delete config '{}': {}", config_name, e),
                                        }
                                    }
                                }
                                _red_button.pop();
                                _disabled_delete.end();
                            }
                            ActiveTab::Info => {
                                let build_info = app.app_state.resolve::<StateBuildInfo>(()).ok();

                                ui.text("An open source CS2 external read only kernel gameplay enhancer.");
                                ui.text(&format!("LABH Version {} ({})", VERSION, env!("BUILD_TIME")));
                                ui.text(&format!(
                                    "CS2 Version {} ({})",
                                    build_info.as_ref().map_or("error", |info| &info.revision),
                                    build_info.as_ref().map_or("error", |info| &info.build_datetime)
                                ));

                                let ydummy = ui.window_size()[1] - ui.cursor_pos()[1] - ui.text_line_height_with_spacing() * 2.0 - 12.0;
                                ui.dummy([0.0, ydummy]);
                                ui.separator();

                                ui.text("Join our discord:");
                                ui.text_colored([0.18, 0.51, 0.97, 1.0], "https://discord.gg/5GteG5yQYd");
                                if ui.is_item_hovered() {
                                    ui.set_mouse_cursor(Some(imgui::MouseCursor::Hand));
                                }

                                if ui.is_item_clicked() {
                                    self.discord_link_copied = Some(Instant::now());
                                    ui.set_clipboard_text("https://discord.gg/5GteG5yQYd");
                                }

                                let show_copied = self.discord_link_copied.as_ref()
                                    .map(|time| time.elapsed().as_millis() < 3_000)
                                    .unwrap_or(false);

                                if show_copied {
                                    ui.same_line();
                                    ui.text("(Copied)");
                                }
                            },
                        }
                    });
            });
    }
    
    fn animated_checkbox(&mut self, ui: &imgui::Ui, label: &str, value: &mut bool) -> bool {
        let _hover_style = ui.push_style_color(StyleColor::FrameBgHovered, [0.0, 0.0, 0.0, 0.0]);
        let clicked = ui.checkbox(label, value);
        _hover_style.pop();
        
        let key = label.to_owned();
        let state = self.checkbox_animations.entry(key).or_insert(WidgetAnimationState { progress: 0.0 });
    
        let speed = 10.0;
        let delta = ui.io().delta_time;
        
        let is_truly_hovered = ui.is_item_hovered() && self.ui_alpha > 0.01;
        
        if is_truly_hovered {
            state.progress = (state.progress + delta * speed).min(1.0);
        } else {
            state.progress = (state.progress - delta * speed).max(0.0);
        }
    
        if state.progress > 0.001 {
            let eased_progress = 1.0f32 - (1.0f32 - state.progress).powi(2);
            let draw_list = ui.get_window_draw_list();
            let original_hover_color = ui.style_color(StyleColor::FrameBgHovered);
            let translucent_hover_color = [original_hover_color[0], original_hover_color[1], original_hover_color[2], 0.4];
    
            let rect_min = ui.item_rect_min();
            let frame_height = ui.frame_height();
            let check_box_max = [rect_min[0] + frame_height, rect_min[1] + frame_height];
            let center = [(rect_min[0] + check_box_max[0]) * 0.5, (rect_min[1] + check_box_max[1]) * 0.5];
            let size = frame_height * eased_progress;
            let anim_min = [center[0] - size * 0.5, center[1] - size * 0.5];
            let anim_max = [center[0] + size * 0.5, center[1] + size * 0.5];
            let frame_rounding = unsafe { ui.style().frame_rounding };
    
            draw_list.add_rect(anim_min, anim_max, translucent_hover_color).filled(true).rounding(frame_rounding).build();
        }
        
        clicked
    }
    
    // --- NEW: Render setting with a toggleable cog that expands a section ---
    fn render_setting_with_cog_toggle(&mut self, app: &Application, ui: &imgui::Ui, label: &str, value: &mut bool, unique_id: &str) -> bool {
        let changed = self.animated_checkbox(ui, label, value);
        ui.same_line();

        let Some(texture_id) = app.resources.cog_texture_id else {
            ui.text("?");
            return false;
        };

        let key = format!("{}_cog", unique_id);
        let state = self.cog_animations.entry(key).or_insert(WidgetAnimationState { progress: 0.0 });
        
        let cog_size = [16.0, 16.0];
        let cursor_pos = ui.cursor_screen_pos();
        let center = [
            cursor_pos[0] + cog_size[0] / 2.0,
            cursor_pos[1] + ui.frame_height() / 2.0 - 1.0,
        ];
        
        let bounding_box_min = [center[0] - cog_size[0] / 2.0, center[1] - cog_size[1] / 2.0];
        let bounding_box_max = [center[0] + cog_size[0] / 2.0, center[1] + cog_size[1] / 2.0];
        
        let is_truly_hovered = ui.is_mouse_hovering_rect(bounding_box_min, bounding_box_max) && self.ui_alpha > 0.01;

        let mut toggle_clicked = false;
        if is_truly_hovered && ui.is_mouse_clicked(imgui::MouseButton::Left) {
            toggle_clicked = true;
        }

        let speed = 8.0;
        let delta = ui.io().delta_time;
        if is_truly_hovered || self.open_dropdowns.contains(&unique_id.to_string()) {
            state.progress = (state.progress + delta * speed).min(1.0);
        } else {
            state.progress = (state.progress - delta * speed).max(0.0);
        }

        let draw_list = ui.get_window_draw_list();
        let eased_progress = 1.0 - (1.0 - state.progress).powi(3);
        let rotation = (90.0f32.to_radians()) * eased_progress; // 90 deg rotation
        let scale = 1.0 + 0.15 * eased_progress;
        
        let (sin_r, cos_r) = (rotation.sin(), rotation.cos());
        let (w, h) = (cog_size[0] * scale / 2.0, cog_size[1] * scale / 2.0);

        let mut corners = [[-w, -h], [ w, -h], [ w,  h], [-w,  h]];

        for p in &mut corners {
            let (x, y) = (p[0], p[1]);
            p[0] = x * cos_r - y * sin_r + center[0];
            p[1] = x * sin_r + y * cos_r + center[1];
        }
        
        let color = if self.open_dropdowns.contains(&unique_id.to_string()) {
             [0.4, 0.7, 1.0, self.ui_alpha] 
        } else { 
             [1.0, 1.0, 1.0, self.ui_alpha] 
        };

        draw_list.add_image_quad(texture_id, corners[0], corners[1], corners[2], corners[3])
            .col(color)
            .build();

        ui.dummy(cog_size);
        
        if toggle_clicked {
             if let Some(pos) = self.open_dropdowns.iter().position(|x| x == unique_id) {
                 self.open_dropdowns.remove(pos);
             } else {
                 self.open_dropdowns.push(unique_id.to_string());
             }
        }
        
        changed
    }

    // --- NEW: The Dropdown Container ---
    fn render_dropdown_section<F>(&mut self, ui: &imgui::Ui, id: &str, content: F) 
    where F: FnOnce(&mut Self, &imgui::Ui) 
    {
        let is_open = self.open_dropdowns.contains(&id.to_string());
        let state = self.dropdown_animations.entry(id.to_string()).or_insert(WidgetAnimationState { progress: 0.0 });
        
        let animation_speed = 12.0;
        let delta = ui.io().delta_time;
        
        if is_open {
            state.progress = (state.progress + delta * animation_speed).min(1.0);
        } else {
            state.progress = (state.progress - delta * animation_speed).max(0.0);
        }

        // Optimization: If completely closed and progress is 0, don't process
        if state.progress < 0.001 {
            return;
        }

        let eased = 1.0 - (1.0 - state.progress).powi(3);
        
        let content_height = *self.dropdown_content_heights.get(id).unwrap_or(&100.0);
        let current_height = content_height * eased;

        let _style = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
        let _border = ui.push_style_var(StyleVar::ChildBorderSize(0.0));
        
        ui.child_window(format!("##dropdown_{}", id))
            .size([0.0, current_height]) // Width 0.0 means auto-fill
            .flags(WindowFlags::NO_SCROLLBAR | WindowFlags::NO_SCROLL_WITH_MOUSE)
            .build(|| {
                ui.indent(); 
                ui.dummy([0.0, 4.0]); 

                let start_y = ui.cursor_pos()[1];
                
                content(self, ui);
                
                let end_y = ui.cursor_pos()[1];
                
                let calculated_height = (end_y - start_y) + 8.0;
                
                if (calculated_height - content_height).abs() > 1.0 {
                    self.dropdown_content_heights.insert(id.to_string(), calculated_height);
                }
                
                ui.unindent(); 
            });
    }

    fn render_esp_settings_player(
        &mut self,
        app: &Application,
        settings: &mut AppSettings,
        ui: &imgui::Ui,
        target: EspSelector,
    ) {
        let config_key = target.config_key();
        
        let config = match settings.esp_settings.entry(config_key.clone()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(EspConfig::Player(EspPlayerSettings::new(&target))),
        };
        let player_config = match config {
            EspConfig::Player(p) => p,
            _ => return,
        };

        // Box
        let mut box_enabled = player_config.box_type != EspBoxType::None;
        if self.render_setting_with_cog_toggle(app, ui, "Box", &mut box_enabled, "box_settings") {
             if box_enabled && player_config.box_type == EspBoxType::None { player_config.box_type = EspBoxType::Box2D; } 
             else if !box_enabled { player_config.box_type = EspBoxType::None; }
        }
        self.render_dropdown_section(ui, "box_settings", |_, ui| {
            ui.combo_enum("Type", &[(EspBoxType::Box2D, "2D"), (EspBoxType::Box3D, "3D")], &mut player_config.box_type);
            Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.box_color);
        });
        
        // Skeleton
        self.render_setting_with_cog_toggle(app, ui, "Skeleton", &mut player_config.skeleton, "skel_settings");
        self.render_dropdown_section(ui, "skel_settings", |_, ui| {
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.skeleton_color);
        });

        // Chams
        self.render_setting_with_cog_toggle(app, ui, "Chams", &mut player_config.chams, "chams_settings");
        ui.same_line();
        ui.text_disabled("(work in progress)");
        self.render_dropdown_section(ui, "chams_settings", |_, ui| {
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.chams_color);
        });

        // Head Dot
        let mut head_dot_enabled = player_config.head_dot != EspHeadDot::None;
        if self.render_setting_with_cog_toggle(app, ui, "Head Dot", &mut head_dot_enabled, "head_settings") {
            if head_dot_enabled && player_config.head_dot == EspHeadDot::None { player_config.head_dot = EspHeadDot::NotFilled; } 
            else if !head_dot_enabled { player_config.head_dot = EspHeadDot::None; }
        }
        self.render_dropdown_section(ui, "head_settings", |_, ui| {
            ui.combo_enum("Type", &[(EspHeadDot::Filled, "Filled"), (EspHeadDot::NotFilled, "Outlined")], &mut player_config.head_dot);
            Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.head_dot_color);
            Self::render_esp_settings_player_style_width(ui, "Z-Offset", 0.0, 10.0, &mut player_config.head_dot_z);
        });

        // Tracers
        let mut tracer_enabled = player_config.tracer_lines != EspTracePosition::None;
        if self.render_setting_with_cog_toggle(app, ui, "Tracer Lines", &mut tracer_enabled, "trace_settings") {
            if tracer_enabled && player_config.tracer_lines == EspTracePosition::None { player_config.tracer_lines = EspTracePosition::BottomCenter; } 
            else if !tracer_enabled { player_config.tracer_lines = EspTracePosition::None; }
        }
        self.render_dropdown_section(ui, "trace_settings", |_, ui| {
            ui.combo_enum("Position", &[ (EspTracePosition::TopLeft, "Top Left"), (EspTracePosition::TopCenter, "Top Center"), (EspTracePosition::TopRight, "Top Right"), (EspTracePosition::BottomLeft, "Bottom Left"), (EspTracePosition::BottomCenter, "Bottom Center"), (EspTracePosition::BottomRight, "Bottom Right")], &mut player_config.tracer_lines);
            Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.tracer_lines_color);
        });

        // Health Bar
        let mut health_bar_enabled = player_config.health_bar != EspHealthBar::None;
        if self.render_setting_with_cog_toggle(app, ui, "Health Bar", &mut health_bar_enabled, "hp_settings") {
            if health_bar_enabled && player_config.health_bar == EspHealthBar::None { player_config.health_bar = EspHealthBar::Left; } 
            else if !health_bar_enabled { player_config.health_bar = EspHealthBar::None; }
        }
        self.render_dropdown_section(ui, "hp_settings", |_, ui| {
             ui.combo_enum("Position", &[(EspHealthBar::Top, "Top"), (EspHealthBar::Left, "Left"), (EspHealthBar::Bottom, "Bottom"), (EspHealthBar::Right, "Right")], &mut player_config.health_bar);
             // ADDED WIDTH SLIDER HERE
             Self::render_esp_settings_player_style_width(ui, "Width", 1.0, 10.0, &mut player_config.health_bar_width);
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_hp_text_color);
        });

        // Text Outline
        self.render_setting_with_cog_toggle(app, ui, "Text Outline", &mut player_config.text_outline_enabled, "outline_settings");
        self.render_dropdown_section(ui, "outline_settings", |_, ui| {
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.text_outline_color);
        });
        
        // Info Group
        self.render_setting_with_cog_toggle(app, ui, "Name", &mut player_config.info_name, "name_settings");
        self.render_dropdown_section(ui, "name_settings", |_, ui| {
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_name_color);
        });

        self.render_setting_with_cog_toggle(app, ui, "Weapon", &mut player_config.info_weapon, "wep_settings");
        self.render_dropdown_section(ui, "wep_settings", |_, ui| {
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_weapon_color);
        });

        self.render_setting_with_cog_toggle(app, ui, "Ammo", &mut player_config.info_ammo, "ammo_settings");
        self.render_dropdown_section(ui, "ammo_settings", |_, ui| {
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_ammo_color);
        });

        self.render_setting_with_cog_toggle(app, ui, "Distance", &mut player_config.info_distance, "dist_settings");
        self.render_dropdown_section(ui, "dist_settings", |_, ui| {
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_distance_color);
        });

        // Individual Flags
        self.render_setting_with_cog_toggle(app, ui, "Kit", &mut player_config.info_flag_kit, "kit_settings");
        self.render_dropdown_section(ui, "kit_settings", |_, ui| {
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_flag_kit_color);
        });

        self.render_setting_with_cog_toggle(app, ui, "Scoped", &mut player_config.info_flag_scoped, "scoped_settings");
        self.render_dropdown_section(ui, "scoped_settings", |_, ui| {
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_flag_scoped_color);
        });

        self.render_setting_with_cog_toggle(app, ui, "Flashed", &mut player_config.info_flag_flashed, "flashed_settings");
        self.render_dropdown_section(ui, "flashed_settings", |_, ui| {
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_flag_flashed_color);
        });

        self.render_setting_with_cog_toggle(app, ui, "Bomb Carrier", &mut player_config.info_flag_bomb, "bomb_settings");
        self.render_dropdown_section(ui, "bomb_settings", |_, ui| {
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_flag_bomb_color);
        });

        // Grenades
        self.render_setting_with_cog_toggle(app, ui, "Grenades", &mut player_config.info_grenades, "nade_settings");
        self.render_dropdown_section(ui, "nade_settings", |_, ui| {
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_grenades_color);
        });

        // Offscreen Arrows
        self.render_setting_with_cog_toggle(app, ui, "Offscreen Arrows", &mut player_config.offscreen_arrows, "arrows_settings");
        self.render_dropdown_section(ui, "arrows_settings", |_, ui| {
             Self::render_esp_settings_player_style_width(ui, "Radius", 50.0, 800.0, &mut player_config.offscreen_arrows_radius);
             Self::render_esp_settings_player_style_width(ui, "Size", 5.0, 40.0, &mut player_config.offscreen_arrows_size);
             Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.offscreen_arrows_color);
        });

        // Near Only
        self.render_setting_with_cog_toggle(app, ui, "Near only", &mut player_config.near_players, "near_settings");
        self.render_dropdown_section(ui, "near_settings", |_, ui| {
             Self::render_esp_settings_player_style_width(ui, "Max Distance", 0.0, 50.0, &mut player_config.near_players_distance);
        });
    }

    // Helper for the standalone cog
    fn render_cog_button(&mut self, app: &Application, ui: &imgui::Ui, unique_id: &str, _is_expanded: &mut bool) -> bool {
         let Some(texture_id) = app.resources.cog_texture_id else { return false; };
         
         let key = format!("{}_cog", unique_id);
         let state = self.cog_animations.entry(key).or_insert(WidgetAnimationState { progress: 0.0 });
         
         let cog_size = [16.0, 16.0];
         let cursor_pos = ui.cursor_screen_pos();
         let center = [
            cursor_pos[0] + cog_size[0] / 2.0,
            cursor_pos[1] + ui.frame_height() / 2.0 - 1.0,
         ];
         
         let bounding_box_min = [center[0] - cog_size[0] / 2.0, center[1] - cog_size[1] / 2.0];
         let bounding_box_max = [center[0] + cog_size[0] / 2.0, center[1] + cog_size[1] / 2.0];
         
         let is_truly_hovered = ui.is_mouse_hovering_rect(bounding_box_min, bounding_box_max) && self.ui_alpha > 0.01;
 
         let mut clicked = false;
         if is_truly_hovered && ui.is_mouse_clicked(imgui::MouseButton::Left) {
            clicked = true;
         }
 
         let speed = 8.0;
         let delta = ui.io().delta_time;
         if is_truly_hovered || self.open_dropdowns.contains(&unique_id.to_string()) {
             state.progress = (state.progress + delta * speed).min(1.0);
         } else {
             state.progress = (state.progress - delta * speed).max(0.0);
         }
 
         let draw_list = ui.get_window_draw_list();
         let eased_progress = 1.0 - (1.0 - state.progress).powi(3);
         let rotation = (90.0f32.to_radians()) * eased_progress;
         let scale = 1.0 + 0.15 * eased_progress;
         
         let (sin_r, cos_r) = (rotation.sin(), rotation.cos());
         let (w, h) = (cog_size[0] * scale / 2.0, cog_size[1] * scale / 2.0);
         let mut corners = [[-w, -h], [ w, -h], [ w,  h], [-w,  h]];
         for p in &mut corners {
             let (x, y) = (p[0], p[1]);
             p[0] = x * cos_r - y * sin_r + center[0];
             p[1] = x * sin_r + y * cos_r + center[1];
         }
         
         let color = if self.open_dropdowns.contains(&unique_id.to_string()) {
             [0.4, 0.7, 1.0, self.ui_alpha] 
         } else { 
             [1.0, 1.0, 1.0, self.ui_alpha] 
         };

         draw_list.add_image_quad(texture_id, corners[0], corners[1], corners[2], corners[3])
            .col(color)
            .build();
         ui.dummy(cog_size);
         
         clicked
    }
    
    fn render_esp_settings_player_style_width(ui: &imgui::Ui, label: &str, min: f32, max: f32, value: &mut f32) {
        ui.text(label);
        ui.same_line();
        ui.set_next_item_width(200.0);
        let _ = ui.slider_config(&format!("##{}_style_width", label), min, max).build(value);
    }

    // --- UPDATED: 2-COLUMN LAYOUT FOR COLOR SETTINGS ---
    fn render_esp_settings_player_style_color(ui: &imgui::Ui, label: &str, color: &mut EspColor) {
        // Start columns with a border to create that "line to the right" effect
        ui.columns(2, format!("cols_{}", label), true); 
        
        // --- COLUMN 1: Type Selector ---
        ui.set_current_column_width(130.0); // Fixed width for the type selector
        
        let mut color_type = EspColorType::from_esp_color(color);
        ui.set_next_item_width(110.0); // Slightly smaller than column width
        
        let color_type_changed = ui.combo_enum(
            &format!("##{}_color_type", label),
            &[
                (EspColorType::Static, "Static"),
                (EspColorType::HealthBased, "Health"),
                (EspColorType::HealthBasedRainbow, "Rainbow"),
                (EspColorType::DistanceBased, "Distance"),
                (EspColorType::GradientPulse, "Pulse"),
                (EspColorType::GradientVertical, "Vertical"),
            ],
            &mut color_type,
        );

        if color_type_changed {
            *color = match color_type {
                EspColorType::Static => EspColor::Static { value: Color::from_f32([1.0, 1.0, 1.0, 1.0]) },
                EspColorType::HealthBased => EspColor::HealthBased { max: Color::from_f32([0.0, 1.0, 0.0, 1.0]), mid: Color::from_f32([1.0, 1.0, 0.0, 1.0]), min: Color::from_f32([1.0, 0.0, 0.0, 1.0]) },
                EspColorType::HealthBasedRainbow => EspColor::HealthBasedRainbow { alpha: 1.0 },
                EspColorType::DistanceBased => EspColor::DistanceBased { near: Color::from_f32([1.0, 0.0, 0.0, 1.0]), mid: Color::from_f32([1.0, 1.0, 0.0, 1.0]), far: Color::from_f32([0.0, 1.0, 0.0, 1.0]) },
                EspColorType::GradientPulse => EspColor::GradientPulse { start: Color::from_f32([1.0, 0.0, 0.0, 1.0]), end: Color::from_f32([0.0, 1.0, 0.0, 1.0]), speed: 1.0 },
                EspColorType::GradientVertical => EspColor::GradientVertical { top: Color::from_f32([1.0, 1.0, 1.0, 1.0]), bottom: Color::from_f32([0.5, 0.5, 0.5, 1.0]) },
            }
        }

        ui.next_column(); // Move to the right side of the line

        // --- COLUMN 2: Controls ---
        match color {
            EspColor::Static { value } => {
                let mut color_value = value.as_f32();
                ui.set_next_item_width(150.0);
                if ui.color_edit4_config(&format!("##{}_static_value", label), &mut color_value).alpha_bar(true).inputs(false).label(false).build() {
                    *value = Color::from_f32(color_value);
                }
            }
            EspColor::HealthBasedRainbow { alpha } => {
                ui.text("Alpha:");
                ui.set_next_item_width(100.0);
                ui.slider_config(&format!("##{}_rainbow_alpha", label), 0.1, 1.0).display_format("%.2f").build(alpha);
            }
            EspColor::HealthBased { max, mid, min } => {
                // Compact layout for 3 colors
                let mut max_value = max.as_f32();
                if ui.color_edit4_config(&format!("##{}_health_max", label), &mut max_value).alpha_bar(true).inputs(false).label(false).build() { *max = Color::from_f32(max_value); }
                if ui.is_item_hovered() { ui.tooltip_text("Max Health Color"); }
                
                ui.same_line();
                let mut mid_value = mid.as_f32();
                if ui.color_edit4_config(&format!("##{}_health_mid", label), &mut mid_value).alpha_bar(true).inputs(false).label(false).build() { *mid = Color::from_f32(mid_value); }
                if ui.is_item_hovered() { ui.tooltip_text("Mid Health Color"); }

                ui.same_line();
                let mut min_value = min.as_f32();
                if ui.color_edit4_config(&format!("##{}_health_min", label), &mut min_value).alpha_bar(true).inputs(false).label(false).build() { *min = Color::from_f32(min_value); }
                if ui.is_item_hovered() { ui.tooltip_text("Min Health Color"); }
            }
            EspColor::DistanceBased { near, mid, far } => {
                let mut near_color = near.as_f32();
                if ui.color_edit4_config(&format!("##{}_near", label), &mut near_color).alpha_bar(true).inputs(false).label(false).build() { *near = Color::from_f32(near_color); }
                if ui.is_item_hovered() { ui.tooltip_text("Near Color"); }

                ui.same_line();
                let mut mid_color = mid.as_f32();
                if ui.color_edit4_config(&format!("##{}_mid", label), &mut mid_color).alpha_bar(true).inputs(false).label(false).build() { *mid = Color::from_f32(mid_color); }
                if ui.is_item_hovered() { ui.tooltip_text("Mid Distance Color"); }

                ui.same_line();
                let mut far_color = far.as_f32();
                if ui.color_edit4_config(&format!("##{}_far", label), &mut far_color).alpha_bar(true).inputs(false).label(false).build() { *far = Color::from_f32(far_color); }
                if ui.is_item_hovered() { ui.tooltip_text("Far Color"); }
            }
            EspColor::GradientPulse { ref mut start, ref mut end, ref mut speed } => {
                let mut s = start.as_f32();
                if ui.color_edit4_config(&format!("##{}_pulse_start", label), &mut s).alpha_bar(true).inputs(false).label(false).build() { *start = Color::from_f32(s); }
                ui.same_line(); 
                let mut e = end.as_f32();
                if ui.color_edit4_config(&format!("##{}_pulse_end", label), &mut e).alpha_bar(true).inputs(false).label(false).build() { *end = Color::from_f32(e); }
                
                ui.set_next_item_width(80.0);
                ui.slider_config(&format!("##{}_pulse_speed", label), 0.1, 10.0).display_format("Spd: %.1f").build(speed);
            }
            EspColor::GradientVertical { ref mut top, ref mut bottom } => {
                let mut t = top.as_f32();
                if ui.color_edit4_config(&format!("##{}_fade_top", label), &mut t).alpha_bar(true).inputs(false).label(false).build() { *top = Color::from_f32(t); }
                ui.same_line(); ui.text("Top");
                
                let mut b = bottom.as_f32();
                if ui.color_edit4_config(&format!("##{}_fade_bot", label), &mut b).alpha_bar(true).inputs(false).label(false).build() { *bottom = Color::from_f32(b); }
                ui.same_line(); ui.text("Bot");
            }
        }

        // Close columns
        ui.columns(1, format!("cols_{}_end", label), false);
    }

    fn render_esp_settings(&mut self, app: &Application, settings: &mut AppSettings, ui: &imgui::Ui) {
        ui.child_window("SettingsPanel")
            .size([350.0, 0.0])
            .build(|| {
                ui.set_next_item_width(150.0);
                ui.combo_enum("Editing Target", &[
                    (PlayerTargetMode::Enemy, "Enemy"),
                    (PlayerTargetMode::Friendly, "Friendly"),
                ], &mut self.esp_player_target_mode);
                ui.separator();

                let target_selector = match self.esp_player_target_mode {
                    PlayerTargetMode::Friendly => EspSelector::PlayerTeam { enemy: false },
                    PlayerTargetMode::Enemy => EspSelector::PlayerTeam { enemy: true },
                };
                
                self.render_esp_settings_player(app, settings, ui, target_selector);
            });

        ui.same_line();

        ui.child_window("PreviewPanel")
            .size([0.0, 0.0])
            .build(|| {
                self.render_esp_preview(app, settings, ui);
            });
    }

    fn render_esp_preview(
        &mut self,
        app: &Application,
        settings: &mut AppSettings,
        ui: &imgui::Ui,
    ) {
        let draw_list = ui.get_window_draw_list();
        let p = ui.cursor_screen_pos();
        let available_size = ui.content_region_avail();
        let alpha = self.ui_alpha;

        let container_pos = [p[0] + 15.0, p[1] + 15.0];
        let container_size = [available_size[0] - 30.0, available_size[1] - 30.0];
        let container_end_pos = [container_pos[0] + container_size[0], container_pos[1] + container_size[1]];

        draw_list.add_rect(container_pos, container_end_pos, [0.07, 0.07, 0.09, 1.0 * alpha])
            .filled(true).rounding(4.0).build();

        let target_selector = match self.esp_player_target_mode {
            PlayerTargetMode::Friendly => EspSelector::PlayerTeam { enemy: false },
            PlayerTargetMode::Enemy => EspSelector::PlayerTeam { enemy: true },
        };
        let config_key = target_selector.config_key();

        let config = settings.esp_settings
            .entry(config_key)
            .or_insert_with(|| EspConfig::Player(EspPlayerSettings::new(&target_selector)));

        if let EspConfig::Player(player_config) = config {
            
            let mut anchor_center = [container_pos[0] + container_size[0] / 2.0, container_pos[1] + container_size[1] / 2.0];
            let mut global_scale = 1.0f32;
            
            // Box dimensions for alignment logic
            let mut box_visual_width = 200.0; 
            let mut box_visual_height = 400.0;

            // --- SCALE CALCULATION ---
            if let Some((_, (w, h))) = app.resources.esp_preview_box_texture_id {
                 let scale_pad = self.preview_layout.global_scale_pad;
                 let img_aspect = w as f32 / h as f32;
                 let container_aspect = container_size[0] / container_size[1];
                 
                 let (draw_w, draw_h) = if img_aspect > container_aspect {
                     (container_size[0] * scale_pad, (container_size[0] * scale_pad) / img_aspect)
                 } else {
                     ((container_size[1] * scale_pad) * img_aspect, container_size[1] * scale_pad)
                 };
 
                 global_scale = draw_w / w as f32;
                 box_visual_width = draw_w;
                 box_visual_height = draw_h;
            } else if let Some((_, (w, h))) = app.resources.character_texture {
                 let scale_pad = self.preview_layout.global_scale_pad;
                 let img_aspect = w as f32 / h as f32;
                 let container_aspect = container_size[0] / container_size[1];
                 let (draw_w, _) = if img_aspect > container_aspect {
                     (container_size[0] * scale_pad, (container_size[0] * scale_pad) / img_aspect)
                 } else {
                     ((container_size[1] * scale_pad) * img_aspect, container_size[1] * scale_pad)
                 };
                 global_scale = draw_w / w as f32;
            }
            
            let box_left = anchor_center[0] - box_visual_width / 2.0;
            let box_right = anchor_center[0] + box_visual_width / 2.0;
            let box_top = anchor_center[1] - box_visual_height / 2.0;

            // Helper: Center an image on the anchor point with optional XY offsets AND SCALE MODIFIER
            let draw_centered = |res: Option<(TextureId, (u32, u32))>, color: [f32; 4], offset_x: f32, offset_y: f32, scale_mod: f32| {
                 if let Some((tid, (orig_w, orig_h))) = res {
                     let mut final_col = color;
                     final_col[3] *= alpha;
                     
                     let effective_scale = global_scale * scale_mod;
                     
                     let item_w = orig_w as f32 * effective_scale;
                     let item_h = orig_h as f32 * effective_scale;
                     
                     let p_min = [
                         anchor_center[0] - item_w / 2.0 + (offset_x * global_scale), 
                         anchor_center[1] - item_h / 2.0 + (offset_y * global_scale)
                     ];
                     let p_max = [p_min[0] + item_w, p_min[1] + item_h];
                     
                     draw_list.add_image(tid, p_min, p_max)
                        .col(final_col)
                        .build();
                 }
            };

            // 1. Draw Character
            if let Some((_, _)) = app.resources.character_texture {
                draw_centered(app.resources.character_texture, [1.0, 1.0, 1.0, 1.0], 
                    self.preview_layout.character_offset[0], self.preview_layout.character_offset[1], 
                    self.preview_layout.character_scale);
            }

            // 2. Draw Box
            if player_config.box_type != EspBoxType::None {
                 let color = player_config.box_color.calculate_color(100.0, 10.0, 0.0, 0.5);
                 draw_centered(app.resources.esp_preview_box_texture_id, color, 0.0, 0.0, 1.0);
            }

            // 3. Skeleton
            if player_config.skeleton {
                let color = player_config.skeleton_color.calculate_color(100.0, 10.0, 0.0, 0.5);
                draw_centered(app.resources.esp_preview_skeleton_texture_id, color, 
                    self.preview_layout.skeleton_offset[0], self.preview_layout.skeleton_offset[1],
                    self.preview_layout.skeleton_scale);
            }

            // 4. Head Dot
            if player_config.head_dot != EspHeadDot::None {
                let color = player_config.head_dot_color.calculate_color(100.0, 10.0, 0.0, 0.5);
                draw_centered(app.resources.esp_preview_head_texture_id, color, 
                    self.preview_layout.head_offset[0], self.preview_layout.head_offset[1],
                    self.preview_layout.head_scale); 
            }

            // 5. Health Bar
            if player_config.health_bar != EspHealthBar::None {
                let color = [0.0, 1.0, 0.0, 1.0]; 
                
                let bar_padding = self.preview_layout.health_bar_padding * global_scale; 
                
                let hb_scale = self.preview_layout.health_bar_scale;

                if let Some((tid, (w, h))) = app.resources.esp_preview_health_lr_texture_id {
                    let bar_w = (w as f32 * global_scale) * hb_scale;
                    let bar_h = (h as f32 * global_scale) * hb_scale;
                    
                    let p_min = [
                        box_left - bar_w - bar_padding, 
                        anchor_center[1] - bar_h / 2.0
                    ];
                    let p_max = [p_min[0] + bar_w, p_min[1] + bar_h];

                    match player_config.health_bar {
                        EspHealthBar::Top | EspHealthBar::Bottom => {
                             if let Some((bt_tid, (bt_w, bt_h))) = app.resources.esp_preview_health_bt_texture_id {
                                let bt_scaled_w = (bt_w as f32 * global_scale) * hb_scale;
                                let bt_scaled_h = (bt_h as f32 * global_scale) * hb_scale;
                                // Bottom bar example
                                let b_min = [anchor_center[0] - bt_scaled_w / 2.0, box_top + box_visual_height + bar_padding];
                                let b_max = [b_min[0] + bt_scaled_w, b_min[1] + bt_scaled_h];
                                let mut final_col = color; final_col[3] *= alpha;
                                draw_list.add_image(bt_tid, b_min, b_max).col(final_col).build();
                             }
                        }
                        _ => {
                             let mut final_col = color; final_col[3] *= alpha;
                             draw_list.add_image(tid, p_min, p_max).col(final_col).build();
                        }
                    }
                }
            }
            
            // 6. Text Info (Name)
            if player_config.info_name {
                let color = player_config.info_name_color.calculate_color(100.0, 10.0, 0.0, 0.5);
                if let Some((tid, (w, h))) = app.resources.esp_preview_name_texture_id {
                    let name_scale = self.preview_layout.name_scale;
                    let item_w = (w as f32 * global_scale) * name_scale;
                    let item_h = (h as f32 * global_scale) * name_scale;
                    
                    let text_padding_x = self.preview_layout.name_padding[0] * global_scale;
                    let text_padding_y = self.preview_layout.name_padding[1] * global_scale;
                    
                    let p_min = [box_right + text_padding_x, box_top + text_padding_y];
                    let p_max = [p_min[0] + item_w, p_min[1] + item_h];
                    
                    let mut final_col = color; final_col[3] *= alpha;
                    draw_list.add_image(tid, p_min, p_max).col(final_col).build();
                }
            }

            if player_config.info_weapon {
                let color = player_config.info_weapon_color.calculate_color(100.0, 10.0, 0.0, 0.5);
                draw_centered(app.resources.esp_preview_gun_texture_id, color, 
                    self.preview_layout.weapon_offset[0], self.preview_layout.weapon_offset[1],
                    self.preview_layout.weapon_scale);
            }

            if player_config.info_distance {
                let color = player_config.info_distance_color.calculate_color(100.0, 10.0, 0.0, 0.5);
                draw_centered(app.resources.esp_preview_distance_texture_id, color, 
                    self.preview_layout.distance_offset[0], self.preview_layout.distance_offset[1],
                    self.preview_layout.distance_scale);
            }

            if player_config.info_ammo {
                 let color = player_config.info_ammo_color.calculate_color(100.0, 10.0, 0.0, 0.5);
                 draw_centered(app.resources.esp_preview_ammo_texture_id, color, 
                    self.preview_layout.ammo_offset[0], self.preview_layout.ammo_offset[1],
                    self.preview_layout.ammo_scale);
            }
        }
    }

    fn render_typewriter_intro(&mut self, ui: &imgui::Ui, app: &Application, elapsed: Duration) {
        let elapsed_s = elapsed.as_secs_f32();
        let display_size = ui.io().display_size;
        
        const STAGE_1_END: f32 = 1.5;
        const STAGE_2_END: f32 = 2.5;
        const STAGE_3_END: f32 = 3.0;
        
        const WINDOW_SIZE: [f32; 2] = [1024.0, 768.0];
        let window_pos = [
            (display_size[0] - WINDOW_SIZE[0]) * 0.5,
            (display_size[1] - WINDOW_SIZE[1]) * 0.5,
        ];

        // Draw window background overlay
        let window_rounding = unsafe { ui.style() }.window_rounding;
        let draw_list = ui.get_background_draw_list();
        draw_list.add_rect(window_pos, [window_pos[0] + WINDOW_SIZE[0], window_pos[1] + WINDOW_SIZE[1]], [0.02, 0.02, 0.03, 1.0])
            .filled(true)
            .rounding(window_rounding)
            .build();

        // Create a transparent window for the text, matching the main GUI size/pos
        ui.window("IntroOverlay")
            .flags(WindowFlags::NO_DECORATION | WindowFlags::NO_INPUTS | WindowFlags::NO_BACKGROUND | WindowFlags::NO_NAV)
            .position(window_pos, Condition::Always)
            .size(WINDOW_SIZE, Condition::Always)
            .build(|| {
                // Use High-Res Intro Font (88px)
                let Some(intro_font_id) = app.fonts.intro.font_id() else { return };
                let _font = ui.push_font(intro_font_id);

                let logo_letters = [
                    ("L", [0.8, 0.8, 0.8]),
                    ("A", [0.7, 0.7, 0.7]),
                    ("B", [0.6, 0.6, 0.6]),
                    ("H", [0.5, 0.5, 0.5]),
                    ("u", [0.4, 0.4, 0.4]),
                    ("b", [0.3, 0.3, 0.3]),
                ];

                // Calculate final position (top-left corner where logo normally is)
                // Relative to screen, matching the main window's title bar position
                // We need to account for the main window's padding (usually 15.0, 15.0)
                let window_padding = unsafe { ui.style() }.window_padding;
                let final_pos = [
                    window_pos[0] + 15.0 + window_padding[0], 
                    window_pos[1] + 8.0 + window_padding[1]
                ];
                const FINAL_SCALE: f32 = 0.25;
                const INITIAL_SCALE: f32 = 1.0;

                // Get the standard spacing from style (usually 8.0)
                let item_spacing_x = unsafe { ui.style() }.item_spacing[0];

                // For stage 1: center of screen
                let center = [display_size[0] / 2.0, display_size[1] / 2.0];
                
                let mut total_width_at_1x = 0.0;
                for (i, (letter, _)) in logo_letters.iter().enumerate() {
                    let letter_width = ui.calc_text_size(letter)[0];
                    total_width_at_1x += letter_width;
                    if i < logo_letters.len() - 1 {
                        // At 1.0 scale (88px font), we want spacing relative to that size.
                        // Since item_spacing_x is for standard font (22px), let's assume 
                        // spacing should scale with font.
                        // item_spacing_x (12.0) corresponds to scale 0.25 (22px).
                        // So at scale 1.0, spacing should be 12.0 / 0.25 = 48.0
                        total_width_at_1x += item_spacing_x / FINAL_SCALE;
                    }
                }
                let total_width_at_start = total_width_at_1x * INITIAL_SCALE;

                // Determine current position and scale based on stage
                let (current_x, current_y, current_scale) = if elapsed_s <= STAGE_1_END {
                    // Stage 1: Centered, 4x scale
                    (center[0] - total_width_at_start / 2.0, center[1] - (ui.text_line_height() * INITIAL_SCALE) / 2.0, INITIAL_SCALE)
                } else if elapsed_s <= STAGE_2_END {
                    // Stage 2: Move and shrink to final position
                    let progress = ((elapsed_s - STAGE_1_END) / (STAGE_2_END - STAGE_1_END)).clamp(0.0, 1.0);
                    let eased = 1.0 - (1.0 - progress).powi(3); // Ease out cubic
                    
                    let start_x = center[0] - total_width_at_start / 2.0;
                    let start_y = center[1] - (ui.text_line_height() * INITIAL_SCALE) / 2.0;
                    
                    let x = start_x + (final_pos[0] - start_x) * eased;
                    let y = start_y + (final_pos[1] - start_y) * eased;
                    let scale = INITIAL_SCALE + (FINAL_SCALE - INITIAL_SCALE) * eased;
                    
                    (x, y, scale)
                } else {
                    // Stage 3: Final position
                    (final_pos[0], final_pos[1], FINAL_SCALE)
                };

                // Set font scale
                ui.set_window_font_scale(current_scale);

                // Render letters
                for (i, (letter, base_color)) in logo_letters.iter().enumerate() {
                    let letter_delay = i as f32 * 0.25; // Each letter appears 0.25s after the previous
                    
                    // Calculate alpha for this letter
                    let alpha = if elapsed_s < letter_delay {
                        0.0
                    } else if elapsed_s < letter_delay + 0.3 {
                        // Fade in over 0.3s
                        ((elapsed_s - letter_delay) / 0.3).clamp(0.0,1.0)
                    } else {
                        1.0
                    };

                    if alpha > 0.01 {
                        let color = [base_color[0], base_color[1], base_color[2], alpha];
                        
                        if i == 0 {
                            ui.set_cursor_screen_pos([current_x, current_y]);
                        } else {
                            // Calculate dynamic spacing so it converges to item_spacing_x at FINAL_SCALE
                            // logic: at scale 0.25 -> spacing 12.0
                            // at scale 1.0 -> spacing 48.0
                            // spacing = item_spacing_x * (current_scale / FINAL_SCALE)
                            let dynamic_spacing = item_spacing_x * (current_scale / FINAL_SCALE);
                            
                            ui.same_line_with_spacing(0.0, dynamic_spacing);
                        }
                        
                        ui.text_colored(color, letter);
                    }
                }
                
                // Reset font scale
                ui.set_window_font_scale(1.0);
            });

        // Stage 3: Fade in main UI (during stage 2 transition)
        if elapsed_s >= 2.0 {
            let fade_progress = ((elapsed_s - 2.0) / (STAGE_3_END - 2.0)).clamp(0.0, 1.0);
            self.ui_alpha = fade_progress.clamp(0.0, 1.0);
        }
    }
}