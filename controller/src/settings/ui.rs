// settings/ui.rs

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
        draw_player_esp, EspRenderInfo,
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
    checkbox_animations: HashMap<String, WidgetAnimationState>,
    cog_animations: HashMap<String, WidgetAnimationState>,
    active_flag_setting: Option<FlagType>,
    ui_alpha: f32,
    is_first_render: bool,
    start_time: Instant,
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
            active_flag_setting: None,
            ui_alpha: 0.0,
            is_first_render: true,
            start_time: Instant::now(),
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
            const INTRO_FADE_DURATION: Duration = Duration::from_millis(1200);
            let elapsed = self.start_time.elapsed();

            if elapsed >= INTRO_FADE_DURATION {
                self.ui_alpha = 1.0;
                self.is_first_render = false;
            } else {
                let progress = elapsed.as_secs_f32() / INTRO_FADE_DURATION.as_secs_f32();
                self.ui_alpha = progress.clamp(0.0, 1.0);
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
                
                let _main_bg = ui.push_style_color(StyleColor::WindowBg, [0.06, 0.05, 0.07, 1.00]);

                {
                    let title_bar_height = 35.0;
                    let _title_bg = ui.push_style_color(StyleColor::ChildBg, [0.07, 0.07, 0.09, 1.0]);

                    ui.child_window("TitleBar")
                        .size([WINDOW_SIZE[0], title_bar_height])
                        .build(|| {
                            let _font = ui.push_font(title_font_id);
                            
                            ui.set_cursor_pos([15.0, 8.0]);
                            for (text, color) in [
                                ("L", [0.8, 0.8, 0.8, 1.0]), ("A", [0.7, 0.7, 0.7, 1.0]),
                                ("B", [0.6, 0.6, 0.6, 1.0]), ("H", [0.5, 0.5, 0.5, 1.0]),
                                ("u", [0.4, 0.4, 0.4, 1.0]), ("b", [0.3, 0.3, 0.3, 1.0]),
                            ] {
                                ui.text_colored(color, text);
                                ui.same_line_with_spacing(0.0, 0.0);
                            }
                        });
                }

                let sidebar_width = 180.0;
                let _sidebar_bg = ui.push_style_color(StyleColor::ChildBg, [0.07, 0.07, 0.09, 1.0]);
                
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
                        self.render_sidebar_button(ui, "Crosshair", font_awesome::CROSSHAIRS, ActiveTab::Crosshair, sidebar_width);
                        
                        render_sidebar_label(ui, "- misc -");
                        self.render_sidebar_button(ui, "Hotkeys", font_awesome::KEYBOARD, ActiveTab::Hotkeys, sidebar_width);
                        self.render_sidebar_button(ui, "Config", font_awesome::SAVE, ActiveTab::Config, sidebar_width);
                        self.render_sidebar_button(ui, "Info", font_awesome::INFO_CIRCLE, ActiveTab::Info, sidebar_width);
                    });

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
                let _content_bg = ui.push_style_color(StyleColor::ChildBg, [0.06, 0.05, 0.07, slide_up_alpha]);

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
    
    fn render_setting_with_cog(&mut self, app: &Application, ui: &imgui::Ui, label: &str, value: &mut bool, popup_id: &str) -> bool {
        let changed = self.animated_checkbox(ui, label, value);
        ui.same_line();

        let Some(texture_id) = app.resources.cog_texture_id else {
            ui.text("?");
            return changed;
        };

        let key = label.to_owned();
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

        if is_truly_hovered && ui.is_mouse_clicked(imgui::MouseButton::Left) {
            ui.open_popup(popup_id);
        }
        if is_truly_hovered {
            ui.tooltip_text("Settings");
        }

        let speed = 8.0;
        let delta = ui.io().delta_time;
        if is_truly_hovered {
            state.progress = (state.progress + delta * speed).min(1.0);
        } else {
            state.progress = (state.progress - delta * speed).max(0.0);
        }

        let draw_list = ui.get_window_draw_list();
        let eased_progress = 1.0 - (1.0 - state.progress).powi(3);

        let rotation = (45.0f32.to_radians()) * eased_progress;
        let scale = 1.0 + 0.15 * eased_progress;
        
        let (sin_r, cos_r) = (rotation.sin(), rotation.cos());
        let (w, h) = (cog_size[0] * scale / 2.0, cog_size[1] * scale / 2.0);

        let mut corners = [[-w, -h], [ w, -h], [ w,  h], [-w,  h]];

        for p in &mut corners {
            let (x, y) = (p[0], p[1]);
            p[0] = x * cos_r - y * sin_r + center[0];
            p[1] = x * sin_r + y * cos_r + center[1];
        }
        
        draw_list.add_image_quad(texture_id, corners[0], corners[1], corners[2], corners[3])
            .col([1.0, 1.0, 1.0, self.ui_alpha])
            .build();

        ui.dummy(cog_size);

        changed
    }

    fn render_flag_setting_with_cog(&mut self, app: &Application, ui: &imgui::Ui, label: &str, value: &mut bool, flag_type: FlagType, popup_id: &str) -> bool {
        let changed = self.animated_checkbox(ui, label, value);
        ui.same_line();

        let Some(texture_id) = app.resources.cog_texture_id else {
            ui.text("?");
            return changed;
        };

        let key = label.to_owned();
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

        if is_truly_hovered && ui.is_mouse_clicked(imgui::MouseButton::Left) {
            self.active_flag_setting = Some(flag_type);
            ui.open_popup(popup_id);
        }
        if is_truly_hovered {
            ui.tooltip_text("Settings");
        }

        let speed = 8.0;
        let delta = ui.io().delta_time;
        if is_truly_hovered {
            state.progress = (state.progress + delta * speed).min(1.0);
        } else {
            state.progress = (state.progress - delta * speed).max(0.0);
        }

        let draw_list = ui.get_window_draw_list();
        let eased_progress = 1.0 - (1.0 - state.progress).powi(3);

        let rotation = (45.0f32.to_radians()) * eased_progress;
        let scale = 1.0 + 0.15 * eased_progress;
        
        let (sin_r, cos_r) = (rotation.sin(), rotation.cos());
        let (w, h) = (cog_size[0] * scale / 2.0, cog_size[1] * scale / 2.0);

        let mut corners = [[-w, -h], [ w, -h], [ w,  h], [-w,  h]];

        for p in &mut corners {
            let (x, y) = (p[0], p[1]);
            p[0] = x * cos_r - y * sin_r + center[0];
            p[1] = x * sin_r + y * cos_r + center[1];
        }
        
        draw_list.add_image_quad(texture_id, corners[0], corners[1], corners[2], corners[3])
            .col([1.0, 1.0, 1.0, self.ui_alpha])
            .build();
        
        ui.dummy(cog_size);

        changed
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

        const POPUP_BOX: &str = "Player Box Settings";
        const POPUP_SKELETON: &str = "Player Skeleton Settings";
        const POPUP_HEAD_DOT: &str = "Head Dot Settings";
        const POPUP_TRACER: &str = "Tracer Lines Settings";
        const POPUP_HEALTH_BAR: &str = "Player Health Bar Settings";
        const POPUP_NAME: &str = "Name Settings";
        const POPUP_WEAPON: &str = "Weapon Settings";
        const POPUP_AMMO: &str = "Ammo Settings";
        const POPUP_DISTANCE: &str = "Distance Settings";
        const POPUP_FLAGS: &str = "Flags Settings";
        const POPUP_GRENADES: &str = "Grenades Settings";
        const POPUP_NEAR_ONLY: &str = "Near Only Settings";

        let mut box_enabled = player_config.box_type != EspBoxType::None;
        if self.render_setting_with_cog(app, ui, "Box", &mut box_enabled, POPUP_BOX) {
            if box_enabled && player_config.box_type == EspBoxType::None { player_config.box_type = EspBoxType::Box2D; } else if !box_enabled { player_config.box_type = EspBoxType::None; }
        }
        
        self.render_setting_with_cog(app, ui, "Skeleton", &mut player_config.skeleton, POPUP_SKELETON);

        let mut head_dot_enabled = player_config.head_dot != EspHeadDot::None;
        if self.render_setting_with_cog(app, ui, "Head Dot", &mut head_dot_enabled, POPUP_HEAD_DOT) {
            if head_dot_enabled && player_config.head_dot == EspHeadDot::None { player_config.head_dot = EspHeadDot::NotFilled; } else if !head_dot_enabled { player_config.head_dot = EspHeadDot::None; }
        }

        let mut tracer_enabled = player_config.tracer_lines != EspTracePosition::None;
        if self.render_setting_with_cog(app, ui, "Tracer Lines", &mut tracer_enabled, POPUP_TRACER) {
            if tracer_enabled && player_config.tracer_lines == EspTracePosition::None { player_config.tracer_lines = EspTracePosition::BottomCenter; } else if !tracer_enabled { player_config.tracer_lines = EspTracePosition::None; }
        }
        
        let mut health_bar_enabled = player_config.health_bar != EspHealthBar::None;
        if self.render_setting_with_cog(app, ui, "Health Bar", &mut health_bar_enabled, POPUP_HEALTH_BAR) {
            if health_bar_enabled && player_config.health_bar == EspHealthBar::None { player_config.health_bar = EspHealthBar::Left; } else if !health_bar_enabled { player_config.health_bar = EspHealthBar::None; }
        }
        
        self.render_setting_with_cog(app, ui, "Name", &mut player_config.info_name, POPUP_NAME);
        self.render_setting_with_cog(app, ui, "Weapon", &mut player_config.info_weapon, POPUP_WEAPON);
        self.render_setting_with_cog(app, ui, "Ammo", &mut player_config.info_ammo, POPUP_AMMO);
        self.render_setting_with_cog(app, ui, "Distance", &mut player_config.info_distance, POPUP_DISTANCE);

        self.render_flag_setting_with_cog(app, ui, "Kit", &mut player_config.info_flag_kit, FlagType::Kit, POPUP_FLAGS);
        self.render_flag_setting_with_cog(app, ui, "Scoped", &mut player_config.info_flag_scoped, FlagType::Scoped, POPUP_FLAGS);
        self.render_flag_setting_with_cog(app, ui, "Flashed", &mut player_config.info_flag_flashed, FlagType::Flashed, POPUP_FLAGS);
        self.render_flag_setting_with_cog(app, ui, "Bomb Carrier", &mut player_config.info_flag_bomb, FlagType::BombCarrier, POPUP_FLAGS);

        self.render_setting_with_cog(app, ui, "Grenades", &mut player_config.info_grenades, POPUP_GRENADES);
        self.render_setting_with_cog(app, ui, "Near only", &mut player_config.near_players, POPUP_NEAR_ONLY);
        
        ui.modal_popup(POPUP_BOX, || {
            ui.separator();
            ui.combo_enum("Type", &[(EspBoxType::Box2D, "2D"), (EspBoxType::Box3D, "3D")], &mut player_config.box_type);
            if let Some(_t) = ui.begin_table_with_sizing("box_settings", 2, imgui::TableFlags::SIZING_STRETCH_SAME, [0.0,0.0], 0.0) {
                Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.box_color);
                Self::render_esp_settings_player_style_width(ui, "Width", 1.0, 10.0, &mut player_config.box_width);
            }
            if ui.button("Close") { ui.close_current_popup(); }
        });

        ui.modal_popup(POPUP_SKELETON, || {
            ui.separator();
            if let Some(_t) = ui.begin_table_with_sizing("skeleton_settings", 2, imgui::TableFlags::SIZING_STRETCH_SAME, [0.0,0.0], 0.0) {
                Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.skeleton_color);
                Self::render_esp_settings_player_style_width(ui, "Width", 1.0, 10.0, &mut player_config.skeleton_width);
            }
            if ui.button("Close") { ui.close_current_popup(); }
        });
        
        ui.modal_popup(POPUP_HEAD_DOT, || {
            ui.separator();
            ui.combo_enum("Type", &[(EspHeadDot::Filled, "Filled"), (EspHeadDot::NotFilled, "Outlined")], &mut player_config.head_dot);
            if let Some(_t) = ui.begin_table_with_sizing("head_dot_settings", 2, imgui::TableFlags::SIZING_STRETCH_SAME, [0.0,0.0], 0.0) {
                Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.head_dot_color);
                Self::render_esp_settings_player_style_width(ui, "Thickness", 1.0, 5.0, &mut player_config.head_dot_thickness);
                Self::render_esp_settings_player_style_width(ui, "Radius", 1.0, 10.0, &mut player_config.head_dot_base_radius);
                Self::render_esp_settings_player_style_width(ui, "Z-Offset", 0.0, 10.0, &mut player_config.head_dot_z);
            }
            if ui.button("Close") { ui.close_current_popup(); }
        });

        ui.modal_popup(POPUP_TRACER, || {
            ui.separator();
            ui.combo_enum("Position", &[ (EspTracePosition::TopLeft, "Top Left"), (EspTracePosition::TopCenter, "Top Center"), (EspTracePosition::TopRight, "Top Right"), (EspTracePosition::BottomLeft, "Bottom Left"), (EspTracePosition::BottomCenter, "Bottom Center"), (EspTracePosition::BottomRight, "Bottom Right")], &mut player_config.tracer_lines);
            if let Some(_t) = ui.begin_table_with_sizing("tracer_settings", 2, imgui::TableFlags::SIZING_STRETCH_SAME, [0.0,0.0], 0.0) {
                Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.tracer_lines_color);
                Self::render_esp_settings_player_style_width(ui, "Width", 1.0, 10.0, &mut player_config.tracer_lines_width);
            }
            if ui.button("Close") { ui.close_current_popup(); }
        });

        ui.modal_popup(POPUP_HEALTH_BAR, || {
            ui.separator();
            ui.combo_enum("Position", &[(EspHealthBar::Top, "Top"), (EspHealthBar::Left, "Left"), (EspHealthBar::Bottom, "Bottom"), (EspHealthBar::Right, "Right")], &mut player_config.health_bar);
            if let Some(_t) = ui.begin_table_with_sizing("health_bar_settings", 2, imgui::TableFlags::SIZING_STRETCH_SAME, [0.0,0.0], 0.0) {
                Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_hp_text_color);
                Self::render_esp_settings_player_style_width(ui, "Width", 5.0, 30.0, &mut player_config.health_bar_width);
            }
            if ui.button("Close") { ui.close_current_popup(); }
        });
        
        ui.modal_popup(POPUP_NAME, || { ui.separator(); if let Some(_t) = ui.begin_table_with_sizing("name_settings", 2, imgui::TableFlags::SIZING_STRETCH_SAME, [0.0,0.0], 0.0) { Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_name_color); } if ui.button("Close") { ui.close_current_popup(); } });
        ui.modal_popup(POPUP_WEAPON, || { ui.separator(); if let Some(_t) = ui.begin_table_with_sizing("weapon_settings", 2, imgui::TableFlags::SIZING_STRETCH_SAME, [0.0,0.0], 0.0) { Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_weapon_color); } if ui.button("Close") { ui.close_current_popup(); } });
        ui.modal_popup(POPUP_AMMO, || { ui.separator(); if let Some(_t) = ui.begin_table_with_sizing("ammo_settings", 2, imgui::TableFlags::SIZING_STRETCH_SAME, [0.0,0.0], 0.0) { Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_ammo_color); } if ui.button("Close") { ui.close_current_popup(); } });
        ui.modal_popup(POPUP_DISTANCE, || { ui.separator(); if let Some(_t) = ui.begin_table_with_sizing("distance_settings", 2, imgui::TableFlags::SIZING_STRETCH_SAME, [0.0,0.0], 0.0) { Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_distance_color); } if ui.button("Close") { ui.close_current_popup(); } });
        
        ui.modal_popup(POPUP_FLAGS, || {
            ui.separator();
            if let Some(flag_type) = self.active_flag_setting {
                let (label, color_target) = match flag_type {
                    FlagType::Kit => ("Kit Color", &mut player_config.info_flag_kit_color),
                    FlagType::Scoped => ("Scoped Color", &mut player_config.info_flag_scoped_color),
                    FlagType::Flashed => ("Flashed Color", &mut player_config.info_flag_flashed_color),
                    FlagType::BombCarrier => ("Bomb Carrier Color", &mut player_config.info_flag_bomb_color),
                };
                
                if let Some(_t) = ui.begin_table_with_sizing("flags_settings", 2, imgui::TableFlags::SIZING_STRETCH_SAME, [0.0,0.0], 0.0) {
                    Self::render_esp_settings_player_style_color(ui, label, color_target);
                }
            }
            if ui.button("Close") { ui.close_current_popup(); }
        });
        
        ui.modal_popup(POPUP_GRENADES, || { ui.separator(); if let Some(_t) = ui.begin_table_with_sizing("grenades_settings", 2, imgui::TableFlags::SIZING_STRETCH_SAME, [0.0,0.0], 0.0) { Self::render_esp_settings_player_style_color(ui, "Color", &mut player_config.info_grenades_color); } if ui.button("Close") { ui.close_current_popup(); } });
        ui.modal_popup(POPUP_NEAR_ONLY, || { ui.separator(); if let Some(_t) = ui.begin_table_with_sizing("near_settings", 2, imgui::TableFlags::SIZING_STRETCH_SAME, [0.0,0.0], 0.0) { Self::render_esp_settings_player_style_width(ui, "Max Distance", 0.0, 50.0, &mut player_config.near_players_distance); } if ui.button("Close") { ui.close_current_popup(); } });
    }
    
    fn render_esp_settings_player_style_width(ui: &imgui::Ui, label: &str, min: f32, max: f32, value: &mut f32) {
        ui.table_next_row();
        ui.table_next_column();
        ui.text(label);
        ui.table_next_column();
        ui.set_next_item_width(-1.0);
        let _ = ui.slider_config(&format!("##{}_style_width", label), min, max).build(value);
    }

    fn render_esp_settings_player_style_color(ui: &imgui::Ui, label: &str, color: &mut EspColor) {
        ui.table_next_row();
        ui.table_next_column();
        ui.text("Type");
        ui.table_next_column();

        let mut color_type = EspColorType::from_esp_color(color);
        ui.set_next_item_width(ui.content_region_avail()[0]);
        let color_type_changed = ui.combo_enum(
            &format!("##{}_color_type", label),
            &[
                (EspColorType::Static, "Static"),
                (EspColorType::HealthBased, "Health based"),
                (EspColorType::HealthBasedRainbow, "Rainbow"),
                (EspColorType::DistanceBased, "Distance"),
            ],
            &mut color_type,
        );

        if color_type_changed {
            *color = match color_type {
                EspColorType::Static => EspColor::Static { value: Color::from_f32([1.0, 1.0, 1.0, 1.0]) },
                EspColorType::HealthBased => EspColor::HealthBased { max: Color::from_f32([0.0, 1.0, 0.0, 1.0]), mid: Color::from_f32([1.0, 1.0, 0.0, 1.0]), min: Color::from_f32([1.0, 0.0, 0.0, 1.0]) },
                EspColorType::HealthBasedRainbow => EspColor::HealthBasedRainbow { alpha: 1.0 },
                EspColorType::DistanceBased => EspColor::DistanceBased { near: Color::from_f32([1.0, 0.0, 0.0, 1.0]), mid: Color::from_f32([1.0, 1.0, 0.0, 1.0]), far: Color::from_f32([0.0, 1.0, 0.0, 1.0]) },
            }
        }
        
        ui.table_next_row();
        ui.table_next_column();
        ui.text("Color");
        ui.table_next_column();

        match color {
            EspColor::Static { value } => {
                let mut color_value = value.as_f32();
                if ui.color_edit4_config(&format!("##{}_static_value", label), &mut color_value).alpha_bar(true).inputs(false).label(false).build() {
                    *value = Color::from_f32(color_value);
                }
            }
            EspColor::HealthBasedRainbow { alpha } => {
                ui.text("Alpha:");
                ui.same_line();
                ui.set_next_item_width(100.0);
                ui.slider_config(&format!("##{}_rainbow_alpha", label), 0.1, 1.0).display_format("%.2f").build(alpha);
            }
            EspColor::HealthBased { max, mid, min } => {
                let mut max_value = max.as_f32();
                if ui.color_edit4_config(&format!("##{}_health_max", label), &mut max_value).alpha_bar(true).inputs(false).label(false).build() { *max = Color::from_f32(max_value); }
                ui.same_line(); ui.text(" => "); ui.same_line();
                let mut mid_value = mid.as_f32();
                if ui.color_edit4_config(&format!("##{}_health_mid", label), &mut mid_value).alpha_bar(true).inputs(false).label(false).build() { *mid = Color::from_f32(mid_value); }
                ui.same_line(); ui.text(" => "); ui.same_line();
                let mut min_value = min.as_f32();
                if ui.color_edit4_config(&format!("##{}_health_min", label), &mut min_value).alpha_bar(true).inputs(false).label(false).build() { *min = Color::from_f32(min_value); }
            }
            EspColor::DistanceBased { near, mid, far } => {
                let mut near_color = near.as_f32();
                if ui.color_edit4_config(&format!("##{}_near", label), &mut near_color).alpha_bar(true).inputs(false).label(false).build() { *near = Color::from_f32(near_color); }
                ui.same_line(); ui.text(" => "); ui.same_line();
                let mut mid_color = mid.as_f32();
                if ui.color_edit4_config(&format!("##{}_mid", label), &mut mid_color).alpha_bar(true).inputs(false).label(false).build() { *mid = Color::from_f32(mid_color); }
                ui.same_line(); ui.text(" => "); ui.same_line();
                let mut far_color = far.as_f32();
                if ui.color_edit4_config(&format!("##{}_far", label), &mut far_color).alpha_bar(true).inputs(false).label(false).build() { *far = Color::from_f32(far_color); }
            }
        }
    }

    fn get_static_preview_bones(
        &self,
        render_pos: [f32; 2],
        render_size: [f32; 2],
    ) -> HashMap<String, [f32; 2]> {
        let scale = 0.65; 
        let vertical_nudge_up = 50.0;

        let scaled_size = [render_size[0] * scale, render_size[1] * scale];
        let offset_x = render_pos[0] + (render_size[0] - scaled_size[0]) / 2.0;
        let offset_y = render_pos[1] + (render_size[1] - scaled_size[1]) / 2.0 - vertical_nudge_up;
        
        let relative_points: HashMap<&str, (f32, f32)> = [
            ("head",        (0.47, 0.26)),
            ("neck_0",      (0.50, 0.25)),
            ("spine_1",     (0.50, 0.38)),
            ("spine_2",     (0.50, 0.50)),
            ("pelvis",      (0.50, 0.55)),
            ("arm_upper_L", (0.35, 0.38)),
            ("arm_lower_L", (0.25, 0.50)),
            ("hand_L",      (0.20, 0.60)),
            ("arm_upper_R", (0.65, 0.38)),
            ("arm_lower_R", (0.75, 0.50)),
            ("hand_R",      (0.80, 0.60)),
            ("leg_upper_L", (0.40, 0.70)),
            ("leg_lower_L", (0.35, 0.85)),
            ("ankle_L",     (0.32, 0.98)),
            ("leg_upper_R", (0.60, 0.70)),
            ("leg_lower_R", (0.65, 0.85)),
            ("ankle_R",     (0.68, 0.98)),
        ].into_iter().collect();

        relative_points.into_iter().map(|(name, (rel_x, rel_y))| {
            let absolute_pos = [
                offset_x + rel_x * scaled_size[0],
                offset_y + rel_y * scaled_size[1],
            ];
            (name.to_string(), absolute_pos)
        }).collect()
    }

    fn render_esp_preview(
        &mut self,
        app: &Application,
        settings: &mut AppSettings,
        ui: &imgui::Ui,
    ) {
        let horizontal_padding = 10.0;
        let vertical_padding = 40.0;
        let x_offset = 0.0;
        let y_offset = -10.0;
    
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
            let bones = self.get_static_preview_bones(container_pos, container_size);
    
            let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for pos in bones.values() {
                min_x = min_x.min(pos[0]);
                min_y = min_y.min(pos[1]);
                max_x = max_x.max(pos[0]);
                max_y = max_y.max(pos[1]);
            }
        
            if min_x != f32::MAX {
                if let Some((texture_id, _)) = app.resources.character_texture {
                    let image_start_pos = [min_x - horizontal_padding + x_offset, min_y - vertical_padding + y_offset];
                    let image_end_pos = [max_x + horizontal_padding + x_offset, max_y + vertical_padding + y_offset];
    
                    draw_list.add_image(texture_id, image_start_pos, image_end_pos)
                        .col([1.0, 1.0, 1.0, 0.6 * alpha])
                        .build();
                }
            }
        
            let render_info = EspRenderInfo {
                bones: &bones, health: 1.0, distance: 5.0, name: "...",
                weapon_name: "...", team_indicator: "...", is_scoped: false,
                is_flashed: false, has_kit: false, has_bomb: false,
            };
    
            let mut preview_player_config = *player_config;
            preview_player_config.head_dot_base_radius *= 7.5;
        
            draw_player_esp(
                &draw_list,
                ui,
                &preview_player_config,
                &render_info,
                container_pos,
                container_size,
                alpha,
                &app.resources, 
            );
        }
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
}