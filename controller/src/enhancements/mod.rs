use crate::settings::AppSettings;
use crate::UpdateContext;
use overlay::UnicodeTextRenderer;
use utils_state::StateRegistry;

pub trait Enhancement {
    fn update(&mut self, ctx: &UpdateContext) -> anyhow::Result<()>;
    fn update_settings(
        &mut self,
        _ui: &imgui::Ui,
        _settings: &mut AppSettings,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }

    fn render(
        &mut self,
        states: &StateRegistry,
        ui: &imgui::Ui,
        unicode_text: &UnicodeTextRenderer,
    ) -> anyhow::Result<()>;
    fn render_debug_window(
        &mut self,
        _states: &StateRegistry,
        _ui: &imgui::Ui,
        _unicode_text: &UnicodeTextRenderer,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

mod bomb;
pub use bomb::*;

mod player;
pub use player::*;

mod trigger;
pub use trigger::*;

mod spectators_list;
pub use spectators_list::*;

mod aim;
pub use aim::*;

mod grenade_helper;
pub use grenade_helper::*;

mod sniper_crosshair;
pub use sniper_crosshair::*;

// ADDED: New module for grenade trajectories
mod grenade_trajectory;
pub use grenade_trajectory::*;

// ADDED: Map parser for physics
pub mod map_loader;

mod legit_aim;
pub use legit_aim::*;