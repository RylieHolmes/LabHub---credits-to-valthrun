// controller\src\main.rs

use image::GenericImageView;
use imgui::TextureId;

use std::{
    cell::{
        Ref,
        RefCell,
        RefMut,
    },
    error::Error,
    fmt::Debug,
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{
            AtomicBool,
            Ordering,
        },
        Arc,
    },
    time::{
        Duration,
        Instant,
    },
};

use anyhow::Context;
use clap::Parser;
use cs2::{
    CS2Handle,
    ConVars,
    InterfaceError,
    StateBuildInfo,
    StateCS2Handle,
    StateCS2Memory,
};
use enhancements::{
    Enhancement,
    GrenadeHelper,
};
use imgui::{
    Condition,
    FontConfig,
    FontGlyphRanges,
    FontId,
    FontSource,
    Key,
    StyleColor,
    Ui,
};
use obfstr::obfstr;
use overlay::{
    LoadingError,
    OverlayError,
    OverlayOptions,
    OverlayTarget,
    SystemRuntimeController,
    UnicodeTextRenderer,
    VulkanError,
};
use settings::{
    load_app_settings,
    AppSettings,
    SettingsUI,
};
use tokio::runtime;
use utils::show_critical_error;
use utils_state::StateRegistry;
use view::ViewController;
use windows::Win32::UI::Shell::IsUserAnAdmin;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState,
    VIRTUAL_KEY,
};
use crate::{
    enhancements::{
        AntiAimPunsh,
        BombInfoIndicator,
        BombLabelIndicator,
        PlayerESP,
        SpectatorsListIndicator,
        TriggerBot,
        SniperCrosshair,
    },
    settings::{
        save_app_settings,
        HotKey,
    },
    utils::TextWithShadowUi,
    winver::version_info,
};
use renderer_3d::Renderer3D;

mod dialog;
mod enhancements;
mod renderer_3d;
mod settings;
mod utils;
mod view;
mod winver;

pub trait MetricsClient {
    fn add_metrics_record(&self, record_type: &str, record_payload: &str);
}

impl MetricsClient for CS2Handle {
    fn add_metrics_record(&self, record_type: &str, record_payload: &str) {
        self.add_metrics_record(record_type, record_payload)
    }
}

pub trait KeyboardInput {
    fn is_key_down(&self, key: imgui::Key) -> bool;
    fn is_key_pressed(&self, key: imgui::Key, repeating: bool) -> bool;
}

impl KeyboardInput for imgui::Ui {
    fn is_key_down(&self, key: imgui::Key) -> bool {
        Ui::is_key_down(self, key)
    }

    fn is_key_pressed(&self, key: imgui::Key, repeating: bool) -> bool {
        if repeating {
            Ui::is_key_pressed(self, key)
        } else {
            Ui::is_key_pressed_no_repeat(self, key)
        }
    }
}

pub struct UpdateContext<'a> {
    pub input: &'a dyn KeyboardInput,
    pub states: &'a StateRegistry,
    pub cs2: &'a Arc<CS2Handle>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FontReference {
    inner: Arc<RefCell<Option<FontId>>>,
}

impl FontReference {
    pub fn font_id(&self) -> Option<FontId> {
        self.inner.borrow().clone()
    }
    pub fn set_id(&self, font_id: FontId) {
        *self.inner.borrow_mut() = Some(font_id);
    }
}

#[derive(Clone, Default)]
pub struct AppFonts {
    labh: FontReference,
    title: FontReference,
}

#[derive(Default)]
pub struct AppResources {
    pub cog_texture_id: Option<TextureId>,
    pub character_texture: Option<(TextureId, (u32, u32))>,
    pub esp_box_texture_id: Option<TextureId>,
    pub esp_skeleton_texture_id: Option<TextureId>,
    pub esp_health_bar_texture_id: Option<TextureId>,
    pub esp_head_dot_texture_id: Option<TextureId>,
}

pub struct Application {
    pub fonts: AppFonts,
    pub resources: AppResources,
    pub renderer_3d: Renderer3D,
    pub app_state: StateRegistry,
    pub cs2: Arc<CS2Handle>,
    pub enhancements: Vec<Rc<RefCell<dyn Enhancement>>>,
    pub frame_read_calls: usize,
    pub last_total_read_calls: usize,
    pub settings_visible: bool,
    pub settings_visibility_changed: AtomicBool,
    pub settings_key_warning_visible: RefCell<bool>,
    pub settings_dirty: bool,
    pub settings_ui: RefCell<SettingsUI>,
    pub settings_screen_capture_changed: AtomicBool,
    pub settings_render_debug_window_changed: AtomicBool,
    pub menu_key_was_down: bool,
    pub is_initialized: AtomicBool,
}

impl Application {
    pub fn settings(&self) -> Ref<'_, AppSettings> {
        self.app_state.get::<AppSettings>(()).expect("app settings to be present")
    }

    pub fn settings_mut(&self) -> RefMut<'_, AppSettings> {
        self.app_state.get_mut::<AppSettings>(()).expect("app settings to be present")
    }

    pub fn load_settings_from_path(&self, path: PathBuf) {
        log::info!("Attempting to load settings from: {:?}", path);
        match std::fs::read_to_string(path) {
            Ok(file_contents) => {
                match serde_yaml::from_str::<AppSettings>(&file_contents) {
                    Ok(new_settings) => { *self.settings_mut() = new_settings; log::info!("Settings loaded successfully."); }
                    Err(e) => { log::error!("Failed to parse config file: {}", e); }
                }
            }
            Err(e) => { log::error!("Failed to read config file: {}", e); }
        }
    }

    pub fn save_settings_to_path(&self, path: PathBuf) {
        log::info!("Attempting to save settings to: {:?}", path);
        match serde_yaml::to_string(&*self.settings()) {
            Ok(yaml_string) => {
                if let Err(e) = std::fs::write(path, yaml_string) { log::error!("Failed to write config file: {}", e); } 
                else { log::info!("Settings saved successfully."); }
            }
            Err(e) => { log::error!("Failed to serialize settings: {}", e); }
        }
    }

    pub fn reset_settings(&self) {
        log::info!("Resetting settings to default.");
        *self.settings_mut() = AppSettings::default();
    }

    pub fn pre_update(&mut self, controller: &mut SystemRuntimeController) -> anyhow::Result<()> {
        if self.settings_dirty {
            self.settings_dirty = false;
            let mut settings = self.settings_mut();

            settings.imgui = None;
            if let Ok(value) = serde_json::to_string(&*settings) { self.cs2.add_metrics_record("settings-updated", &value); }

            let mut imgui_settings = String::new();
            controller.imgui.save_ini_settings(&mut imgui_settings);
            settings.imgui = Some(imgui_settings);

            if let Err(error) = save_app_settings(&*settings) { log::warn!("Failed to save user settings: {}", error); };
        }

        controller.set_passthrough(!self.settings_visible);

        if self.settings_screen_capture_changed.swap(false, Ordering::Relaxed) {
            let settings = self.settings();
            controller.toggle_screen_capture_visibility(!settings.hide_overlay_from_screen_capture);
            log::debug!("Updating screen capture visibility to {}", !settings.hide_overlay_from_screen_capture);
        }

        if self.settings_render_debug_window_changed.swap(false, Ordering::Relaxed) {
            let settings = self.settings();
            controller.toggle_debug_overlay(settings.render_debug_window);
        }

        Ok(())
    }

    pub fn update(&mut self, ui: &imgui::Ui) -> anyhow::Result<()> {
        for enhancement in self.enhancements.iter() {
            let mut hack = enhancement.borrow_mut();
            if hack.update_settings(ui, &mut *self.settings_mut())? { self.settings_dirty = true; }
        }

        let menu_key = self.settings().key_settings.0;

        let vk_menu_key = map_imgui_key_to_vk(menu_key);
        let menu_key_is_down = if vk_menu_key.0 != 0 {
            unsafe { (GetAsyncKeyState(vk_menu_key.0 as i32) as u16 & 0x8000) != 0 }
        } else {
            ui.is_key_down(menu_key)
        };

        if menu_key_is_down && !self.menu_key_was_down {
            log::debug!("Toggle settings");
            self.settings_visible = !self.settings_visible;
            self.settings_visibility_changed.store(true, Ordering::Relaxed);
            self.cs2.add_metrics_record("settings-toggled", &format!("visible: {}", self.settings_visible));

            if !self.settings_visible { self.settings_dirty = true; }
        }
        self.menu_key_was_down = menu_key_is_down;

        self.app_state.invalidate_states();
        if let Ok(mut view_controller) = self.app_state.resolve_mut::<ViewController>(()) {
            view_controller.update_screen_bounds(mint::Vector2::from_slice(&ui.io().display_size));
        }

        let update_context = UpdateContext { cs2: &self.cs2, states: &self.app_state, input: ui };

        for enhancement in self.enhancements.iter() {
            let mut enhancement = enhancement.borrow_mut();
            enhancement.update(&update_context)?;
        }

        let read_calls = self.cs2.ke_interface.total_read_calls();
        self.frame_read_calls = read_calls - self.last_total_read_calls;
        self.last_total_read_calls = read_calls;

        Ok(())
    }

    pub fn render(&mut self, ui: &imgui::Ui, unicode_text: &UnicodeTextRenderer) {
        if !self.is_initialized.load(Ordering::Relaxed) {
            return;
        }

        ui.window("overlay").draw_background(false).no_decoration().no_inputs().size(ui.io().display_size, Condition::Always).position([0.0, 0.0], Condition::Always).build(|| self.render_overlay(ui, unicode_text));

        for enhancement in self.enhancements.iter() {
            let mut enhancement = enhancement.borrow_mut();
            if let Err(err) = enhancement.render_debug_window(&self.app_state, ui, unicode_text) { log::error!("{:?}", err); }
        }

        let mut settings_ui = self.settings_ui.borrow_mut();
        settings_ui.render(self, ui, unicode_text);

        let mut warning_visible = self.settings_key_warning_visible.borrow_mut();
        self.render_settings_key_warning(ui, &mut *warning_visible);
    }

    fn render_settings_key_warning(&self, ui: &imgui::Ui, popup_visible: &mut bool) {
        if !*popup_visible { return; }

        let mut settings = self.settings_mut();
        let display_size = ui.io().display_size;
        ui.window("##warning_insert_key").movable(false).collapsible(false).always_auto_resize(true).position([display_size[0] * 0.5, display_size[1] * 0.5], Condition::Always).position_pivot([0.5, 0.5]).build(|| {
            ui.text("We detected you pressed the \"INSERT\" key.");
            ui.text("If you meant to open the LABH Overlay please use the \"PAUSE\" key.");
            ui.dummy([0.0, 2.5]);
            ui.separator();
            ui.dummy([0.0, 2.5]);

            ui.set_next_item_width(ui.content_region_avail()[0]);
            ui.checkbox("Do not show this warning again", &mut settings.key_settings_ignore_insert_warning);

            ui.dummy([0.0, 2.5]);
            if ui.button("Bind to INSERT") {
                settings.key_settings = HotKey(Key::Insert);
                *popup_visible = false;
            }

            ui.same_line_with_pos(ui.content_region_avail()[0] - 100.0);
            if ui.button_with_size("Close", [100.0, 0.0]) { *popup_visible = false; }
        });
    }

    fn render_overlay(&self, ui: &imgui::Ui, unicode_text: &UnicodeTextRenderer) {
        let settings = self.settings();
        let window_size = ui.window_size();

        if settings.labh_watermark {
            {
                let text_buf;
                let text = obfstr!(text_buf = "LABH Overlay");
                let text_size = ui.calc_text_size(text);
                ui.set_cursor_pos([window_size[0] - text_size[0] - 10.0, 10.0]);
                ui.text_with_shadow(text);
            }
            {
                let text = format!("{:.2} FPS", ui.io().framerate);
                let text_size = ui.calc_text_size(&text);
                ui.set_cursor_pos([window_size[0] - text_size[0] - 10.0, 24.0]);
                ui.text_with_shadow(&text)
            }
            {
                let text = format!("{} Reads", self.frame_read_calls);
                let text_size = ui.calc_text_size(&text);
                ui.set_cursor_pos([window_size[0] - text_size[0] - 10.0, 38.0]);
                ui.text_with_shadow(&text)
            }
        }

        for enhancement in self.enhancements.iter() {
            let mut hack = enhancement.borrow_mut();
            if let Err(err) = hack.render(&self.app_state, ui, unicode_text) { log::error!("{:?}", err); }
        }
    }
}

fn map_imgui_key_to_vk(key: imgui::Key) -> VIRTUAL_KEY {
    let vk_code = match key {
        Key::Insert => 0x2D,
        Key::Pause => 0x13,
        _ => 0,
    };
    VIRTUAL_KEY(vk_code as u16)
}

fn main() {
    let args = match AppArgs::try_parse() {
        Ok(args) => args,
        Err(error) => { println!("{:#}", error); std::process::exit(1); }
    };

    env_logger::builder().filter_level(if args.verbose { log::LevelFilter::Trace } else { log::LevelFilter::Info }).parse_default_env().init();
    let runtime = runtime::Builder::new_multi_thread().enable_all().worker_threads(1).build().expect("to be able to build a runtime");
    let _runtime_guard = runtime.enter();
    if let Err(error) = real_main(&args) { show_critical_error(&format!("{:#}", error)); }
}

#[derive(Debug, Parser)]
#[clap(name = "LABH", version)]
struct AppArgs {
    #[clap(short, long)]
    verbose: bool,
    #[arg(short, long)]
    schema_file: Option<PathBuf>,
}

fn real_main(args: &AppArgs) -> anyhow::Result<()> {
    let build_info = version_info()?;
    log::info!("{} v{} ({}). Windows build {}.", obfstr!("LABH"), env!("CARGO_PKG_VERSION"), env!("GIT_HASH"), build_info.dwBuildNumber);
    log::info!("{} {}", obfstr!("Current executable was built on"), env!("BUILD_TIME"));

    if unsafe { IsUserAnAdmin().as_bool() } {
        log::warn!("{}", obfstr!("Please do not run this as administrator!"));
        log::warn!("{}", obfstr!("Running the controller as administrator might cause failures with your graphic drivers."));
    }

    let settings = load_app_settings()?;
    let cs2 = match CS2Handle::create(settings.metrics) {
        Ok(handle) => handle,
        Err(err) => {
            if let Some(err) = err.downcast_ref::<InterfaceError>() {
                if let Some(detailed_message) = err.detailed_message() {
                    show_critical_error(&detailed_message);
                    return Ok(());
                }
            }
            return Err(err);
        }
    };

    let driver_name = cs2.ke_interface.driver_version().get_application_name().unwrap_or("<invalid>");
    if driver_name == obfstr!("zenith-driver") {
        let message = [obfstr!("You are using Zenith with the CS2 overlay."), obfstr!("Topmost overlays may be flagged regardless of using the Zenith driver."), obfstr!(""), obfstr!("Do you want to continue?")].join("\n");
        let result = dialog::show_yes_no(obfstr!("LABH"), &message, false);
        if !result { log::info!("{}", obfstr!("Aborting launch due to user input.")); return Ok(()); }
    }

    cs2.add_metrics_record(obfstr!("controller-status"), "initializing");

    let mut app_state = StateRegistry::new(1024 * 8);
    app_state.set(StateCS2Handle::new(cs2.clone()), ())?;
    app_state.set(StateCS2Memory::new(cs2.create_memory_view()), ())?;
    app_state.set(settings, ())?;

    {
        let cs2_build_info = app_state.resolve::<StateBuildInfo>(()).context(obfstr!("Failed to load CS2 build info. CS2 version might be newer / older then expected").to_string())?;
        log::info!("Found {}. Revision {} from {}.", obfstr!("Counter-Strike 2"), cs2_build_info.revision, cs2_build_info.build_datetime);
        cs2.add_metrics_record(obfstr!("cs2-version"), &format!("revision: {}", cs2_build_info.revision));
    }

    if let Some(file) = &args.schema_file {
        log::info!("{} {}", obfstr!("Loading CS2 schema (offsets) from file"), file.display());
        cs2_schema_provider_impl::setup_schema_from_file(&mut app_state, file).context("file schema setup")?;
    } else {
        log::info!("{}", obfstr!("Loading CS2 schema (offsets) from CS2 schema system"));
        cs2_schema_provider_impl::setup_provider(Box::new(cs2_schema_provider_impl::RuntimeSchemaProvider::new(&app_state).context("load runtime schema")?));
    }
    log::info!("CS2 schema (offsets) loaded.");

    let cvars = ConVars::new(&app_state).context("cvars")?;
    let cvar_sensitivity = cvars.find_cvar("sensitivity").context("cvar sensitivity")?.context("missing cvar sensitivity")?;

    log::debug!("Initialize overlay");
    let app_fonts: AppFonts = Default::default();
    let overlay_options = OverlayOptions {
        title: obfstr!("CS2 Overlay").to_string(),
        target: OverlayTarget::WindowOfProcess(cs2.process_id() as u32),
        register_fonts_callback: Some(Box::new({
            let app_fonts = app_fonts.clone();
            move |atlas| {
                const FA_GLYPH_RANGES: &[u32] = &[0xf000, 0xf3ff, 0, ];
                let font_config = FontConfig { rasterizer_multiply: 1.2, oversample_h: 3, oversample_v: 3, ..FontConfig::default() };
                let poppins_font = atlas.add_font(&[FontSource::TtfData { data: include_bytes!("../resources/Poppins-Regular.ttf"), size_pixels: 16.0, config: Some(font_config.clone()) }, FontSource::TtfData { data: include_bytes!("../resources/fa-solid-900.ttf"), size_pixels: 16.0, config: Some(FontConfig { glyph_ranges: FontGlyphRanges::from_slice(FA_GLYPH_RANGES), ..font_config.clone() }) }]);
                app_fonts.labh.set_id(poppins_font);
                let title_font = atlas.add_font(&[FontSource::TtfData { data: include_bytes!("../resources/Poppins-Regular.ttf"), size_pixels: 22.0, config: Some(FontConfig { rasterizer_multiply: 1.2, oversample_h: 4, oversample_v: 4, ..FontConfig::default() }) }, FontSource::TtfData { data: include_bytes!("../resources/fa-solid-900.ttf"), size_pixels: 22.0, config: Some(FontConfig { glyph_ranges: FontGlyphRanges::from_slice(FA_GLYPH_RANGES), ..font_config.clone() }) }]);
                app_fonts.title.set_id(title_font);
            }
        })),
    };

    let mut overlay = match overlay::init(overlay_options) {
        Err(OverlayError::Vulkan(VulkanError::DllNotFound(LoadingError::LibraryLoadFailure(source)))) => {
            match &source {
                libloading::Error::LoadLibraryExW { .. } => {
                    let error = source.source().context("LoadLibraryExW to have a source")?;
                    let message = format!("Failed to load vulkan-1.dll.\nError: {:#}", error);
                    show_critical_error(&message);
                }
                error => {
                    let message = format!("An error occurred while loading vulkan-1.dll.\nError: {:#}", error);
                    show_critical_error(&message);
                }
            }
            return Ok(());
        }
        value => value?,
    };

    let mut app_resources = AppResources::default();
    {
        const COG_IMAGE_BYTES: &[u8] = include_bytes!("../resources/cog.png");
        let image = image::load_from_memory(COG_IMAGE_BYTES).expect("Failed to load cog.png from resources folder");
        let rgba_image = image.to_rgba8();
        let dimensions = image.dimensions();
        let texture_data = rgba_image.into_raw();
        
        let cog_texture_id = unsafe {
            overlay.add_texture(&texture_data, dimensions.0, dimensions.1)?
        };
        
        app_resources.cog_texture_id = Some(cog_texture_id);
    }
    
    {
        const IMAGE_BYTES: &[u8] = include_bytes!("../resources/box.png");
        let image = image::load_from_memory(IMAGE_BYTES).context("Failed to load box.png")?;
        let rgba_image = image.to_rgba8();
        let dimensions = image.dimensions();
        let texture_data = rgba_image.into_raw();
        app_resources.esp_box_texture_id = Some(unsafe { overlay.add_texture(&texture_data, dimensions.0, dimensions.1)? });
    }

    {
        const IMAGE_BYTES: &[u8] = include_bytes!("../resources/skeleton.png");
        let image = image::load_from_memory(IMAGE_BYTES).context("Failed to load skeleton.png")?;
        let rgba_image = image.to_rgba8();
        let dimensions = image.dimensions();
        let texture_data = rgba_image.into_raw();
        app_resources.esp_skeleton_texture_id = Some(unsafe { overlay.add_texture(&texture_data, dimensions.0, dimensions.1)? });
    }

    {
        const IMAGE_BYTES: &[u8] = include_bytes!("../resources/health_bar.png");
        let image = image::load_from_memory(IMAGE_BYTES).context("Failed to load health_bar.png")?;
        let rgba_image = image.to_rgba8();
        let dimensions = image.dimensions();
        let texture_data = rgba_image.into_raw();
        app_resources.esp_health_bar_texture_id = Some(unsafe { overlay.add_texture(&texture_data, dimensions.0, dimensions.1)? });
    }

    {
        const IMAGE_BYTES: &[u8] = include_bytes!("../resources/head_dot.png");
        let image = image::load_from_memory(IMAGE_BYTES).context("Failed to load head_dot.png")?;
        let rgba_image = image.to_rgba8();
        let dimensions = image.dimensions();
        let texture_data = rgba_image.into_raw();
        app_resources.esp_head_dot_texture_id = Some(unsafe { overlay.add_texture(&texture_data, dimensions.0, dimensions.1)? });
    }
    
    {
        const CHARACTER_IMAGE_BYTES: &[u8] = include_bytes!("../resources/character.png");
        match image::load_from_memory(CHARACTER_IMAGE_BYTES) {
            Ok(image) => {
                let rgba_image = image.to_rgba8();
                let dimensions = image.dimensions();
                let texture_data = rgba_image.into_raw();
                let character_texture_id = unsafe {
                    overlay.add_texture(&texture_data, dimensions.0, dimensions.1)?
                };
                app_resources.character_texture = Some((character_texture_id, dimensions));
                log::info!("Successfully loaded character.png for ESP preview.");
            },
            Err(e) => {
                log::warn!("Could not load resources/character.png for ESP preview: {}. The preview will not show a model.", e);
            }
        }
    }

    // No logo loading logic here anymore

    let renderer_3d = Renderer3D::new("resources/character.glb", &mut overlay)
        .context("Failed to load 3D model data. Ensure 'resources/character.glb' exists.")?;
    log::info!("Successfully loaded character.glb data for 3D ESP preview.");

    apply_custom_style(overlay.imgui.style_mut());

    {
        let settings = app_state.resolve::<AppSettings>(())?;
        if let Some(imgui_settings) = &settings.imgui { overlay.imgui.load_ini_settings(imgui_settings); }
    }
    
    let app = Application {
        fonts: app_fonts,
        resources: app_resources,
        renderer_3d,
        app_state,
        cs2: cs2.clone(),
        enhancements: vec![
            Rc::new(RefCell::new(AntiAimPunsh::new(cvar_sensitivity))),
            Rc::new(RefCell::new(PlayerESP::new())),
            Rc::new(RefCell::new(SpectatorsListIndicator::new())),
            Rc::new(RefCell::new(BombInfoIndicator::new())),
            Rc::new(RefCell::new(BombLabelIndicator::new())),
            Rc::new(RefCell::new(TriggerBot::new())),
            Rc::new(RefCell::new(GrenadeHelper::new())),
            Rc::new(RefCell::new(SniperCrosshair::new())),
        ],
        last_total_read_calls: 0,
        frame_read_calls: 0,
        settings_visible: true,
        settings_visibility_changed: AtomicBool::new(true),
        settings_key_warning_visible: RefCell::new(false),
        settings_dirty: false,
        settings_ui: RefCell::new(SettingsUI::new()),
        settings_screen_capture_changed: AtomicBool::new(true),
        settings_render_debug_window_changed: AtomicBool::new(true),
        menu_key_was_down: false,
        is_initialized: AtomicBool::new(false),
    };
    let app = Rc::new(RefCell::new(app));

    app.borrow().is_initialized.store(true, Ordering::Relaxed);

    cs2.add_metrics_record(obfstr!("controller-status"), &format!("initialized, version: {}, git-hash: {}, win-build: {}", env!("CARGO_PKG_VERSION"), env!("GIT_HASH"), build_info.dwBuildNumber));

    log::info!("{}", obfstr!("App initialized. Spawning overlay."));
    let mut update_fail_count = 0;
    let mut update_timeout: Option<(Instant, Duration)> = None;
    
    overlay.main_loop(
        {
            let app = app.clone();
            move |controller| {
                let mut app = app.borrow_mut();
                if let Err(err) = app.pre_update(controller) { 
                    show_critical_error(&format!("{:#}", err)); 
                    false 
                } else { 
                    true 
                }
            }
        },
        move |ui, unicode_text| {
            let mut app = app.borrow_mut();

            if let Some((timeout, target)) = &update_timeout {
                if timeout.elapsed() > *target { update_timeout = None; } 
                else { return true; }
            }

            if let Err(err) = app.update(ui) {
                if update_fail_count >= 10 {
                    log::error!("Over 10 errors occurred. Waiting 1s and try again.");
                    log::error!("Last error: {:#}", err);
                    update_timeout = Some((Instant::now(), Duration::from_millis(1000)));
                    update_fail_count = 0;
                    return true;
                } else {
                    update_fail_count += 1;
                }
            }

            app.render(ui, unicode_text);
            true
        },
    );

    Ok(())
}

fn apply_custom_style(style: &mut imgui::Style) {
    style.window_padding = [15.0, 15.0];
    style.window_rounding = 5.0;
    style.frame_padding = [5.0, 5.0];
    style.frame_rounding = 4.0;
    style.item_spacing = [12.0, 8.0];
    style.item_inner_spacing = [8.0, 6.0];
    style.indent_spacing = 25.0;
    style.scrollbar_size = 15.0;
    style.scrollbar_rounding = 9.0;
    style.grab_min_size = 5.0;
    style.grab_rounding = 3.0;
    style.tab_rounding = 4.0;
    style.window_title_align = [0.5, 0.5];

    let colors = &mut style.colors;
    colors[StyleColor::Text as usize] = [0.80, 0.80, 0.83, 1.00];
    colors[StyleColor::TextDisabled as usize] = [0.45, 0.45, 0.48, 1.00];
    colors[StyleColor::WindowBg as usize] = [0.06, 0.05, 0.07, 1.00];
    colors[StyleColor::ChildBg as usize] = [0.07, 0.07, 0.09, 1.00];
    colors[StyleColor::PopupBg as usize] = [0.07, 0.07, 0.09, 1.00];
    colors[StyleColor::Border as usize] = [0.80, 0.80, 0.83, 0.88];
    colors[StyleColor::BorderShadow as usize] = [0.92, 0.91, 0.88, 0.00];
    colors[StyleColor::FrameBg as usize] = [0.10, 0.09, 0.12, 1.00];
    colors[StyleColor::FrameBgHovered as usize] = [0.24, 0.23, 0.29, 1.00];
    colors[StyleColor::FrameBgActive as usize] = [0.56, 0.56, 0.58, 1.00];
    colors[StyleColor::TitleBg as usize] = [0.10, 0.09, 0.12, 1.00];
    colors[StyleColor::TitleBgActive as usize] = [0.07, 0.07, 0.09, 1.00];
    colors[StyleColor::TitleBgCollapsed as usize] = [1.00, 0.98, 0.95, 0.75];
    colors[StyleColor::MenuBarBg as usize] = [0.10, 0.09, 0.12, 1.00];
    colors[StyleColor::ScrollbarBg as usize] = [0.10, 0.09, 0.12, 1.00];
    colors[StyleColor::ScrollbarGrab as usize] = [0.80, 0.80, 0.83, 0.31];
    colors[StyleColor::ScrollbarGrabHovered as usize] = [0.56, 0.56, 0.58, 1.00];
    colors[StyleColor::ScrollbarGrabActive as usize] = [0.06, 0.05, 0.07, 1.00];
    colors[StyleColor::CheckMark as usize] = [0.80, 0.80, 0.83, 0.31];
    colors[StyleColor::SliderGrab as usize] = [0.80, 0.80, 0.83, 0.31];
    colors[StyleColor::SliderGrabActive as usize] = [0.06, 0.05, 0.07, 1.00];
    colors[StyleColor::Button as usize] = [0.10, 0.09, 0.12, 1.00];
    colors[StyleColor::ButtonHovered as usize] = [0.24, 0.23, 0.29, 1.00];
    colors[StyleColor::ButtonActive as usize] = [0.56, 0.56, 0.58, 1.00];
    colors[StyleColor::Header as usize] = [0.10, 0.09, 0.12, 1.00];
    colors[StyleColor::HeaderHovered as usize] = [0.56, 0.56, 0.58, 1.00];
    colors[StyleColor::HeaderActive as usize] = [0.06, 0.05, 0.07, 1.00];
    colors[StyleColor::Separator as usize] = [0.43, 0.43, 0.50, 0.50];
    colors[StyleColor::SeparatorHovered as usize] = [0.10, 0.40, 0.75, 0.78];
    colors[StyleColor::SeparatorActive as usize] = [0.10, 0.40, 0.75, 1.00];
    colors[StyleColor::ResizeGrip as usize] = [0.00, 0.00, 0.00, 0.00];
    colors[StyleColor::ResizeGripHovered as usize] = [0.56, 0.56, 0.58, 1.00];
    colors[StyleColor::ResizeGripActive as usize] = [0.06, 0.05, 0.07, 1.00];
    colors[StyleColor::Tab as usize] = [0.10, 0.09, 0.12, 1.00];
    colors[StyleColor::TabHovered as usize] = [0.24, 0.23, 0.29, 1.00];
    colors[StyleColor::TabActive as usize] = [0.14, 0.13, 0.17, 1.00];
    colors[StyleColor::TabUnfocused as usize] = [0.10, 0.09, 0.12, 1.00];
    colors[StyleColor::TabUnfocusedActive as usize] = [0.20, 0.25, 0.29, 1.00];
    colors[StyleColor::TextSelectedBg as usize] = [0.25, 1.00, 0.00, 0.43];
    colors[StyleColor::NavHighlight as usize] = [0.26, 0.59, 0.98, 1.00];
}