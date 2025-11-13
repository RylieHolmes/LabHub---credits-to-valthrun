use imgui::{ImColor32, Ui};
use overlay::UnicodeTextRenderer;
use utils_state::StateRegistry;
use cs2::{
    CEntityIdentityEx,
    StateEntityList,
    LocalCameraControllerTarget,
    StateCS2Memory,
    WeaponId,
};
use cs2_schema_generated::cs2::client::{C_CSPlayerPawn, C_CSPlayerPawnBase, C_EconEntity};

use super::Enhancement;
use crate::{
    settings::AppSettings,
    UpdateContext,
};

#[derive(Default)]
pub struct SniperCrosshair;

impl SniperCrosshair {
    pub fn new() -> Self {
        Self::default()
    }

    fn is_sniper_weapon(&self, weapon_id: u16) -> bool {
        matches!(
            WeaponId::from_id(weapon_id).unwrap_or(WeaponId::Unknown),
            WeaponId::AWP | WeaponId::Ssg08 | WeaponId::Scar20 | WeaponId::G3SG1
        )
    }
}

impl Enhancement for SniperCrosshair {
    fn update(&mut self, _ctx: &UpdateContext) -> anyhow::Result<()> { Ok(()) }

    fn render(&mut self, states: &StateRegistry, ui: &Ui, _unicode_text: &UnicodeTextRenderer) -> anyhow::Result<()> {
        let settings = states.resolve::<AppSettings>(())?;
        if !settings.sniper_crosshair { return Ok(()); }
        
        let style = &settings.sniper_crosshair_settings;

        let Ok(view_target) = states.resolve::<LocalCameraControllerTarget>(()) else { return Ok(()) };
        let Ok(entities) = states.resolve::<StateEntityList>(()) else { return Ok(()) };
        let Ok(memory) = states.resolve::<StateCS2Memory>(()) else { return Ok(()) };
        
        let Some(target_entity_id) = view_target.target_entity_id else { return Ok(()) };
        let Some(pawn_identity) = entities.identity_from_index(target_entity_id) else { return Ok(()) };
        
        if let Some(pawn_base) = pawn_identity.entity_ptr::<dyn C_CSPlayerPawnBase>()?.value_reference(memory.view_arc()) {
            let pawn = pawn_base.cast::<dyn C_CSPlayerPawn>();
            
            let is_sniper = (|| -> anyhow::Result<bool> {
                let Some(weapon_ref) = pawn.m_pClippingWeapon()?.value_reference(memory.view_arc()) else { return Ok(false) };
                let weapon_econ_entity = weapon_ref.cast::<dyn C_EconEntity>();
                let weapon_id = weapon_econ_entity.m_AttributeManager()?.m_Item()?.m_iItemDefinitionIndex()?;
                Ok(self.is_sniper_weapon(weapon_id))
            })().unwrap_or(false);

            if !is_sniper { return Ok(()); }

            let display_size = ui.io().display_size;
            let center = [display_size[0] / 2.0, display_size[1] / 2.0];
            let color = ImColor32::from_rgba(style.color[0], style.color[1], style.color[2], style.color[3]);
            let outline_color = ImColor32::from_rgba(0, 0, 0, style.color[3]);
            let draw_list = ui.get_window_draw_list();
            
            if style.outline {
                let outline_px = style.outline_thickness;
                draw_list.add_line([center[0] - style.gap - style.size, center[1]], [center[0] - style.gap, center[1]], outline_color).thickness(style.thickness + outline_px * 2.0).build();
                draw_list.add_line([center[0] + style.gap, center[1]], [center[0] + style.gap + style.size, center[1]], outline_color).thickness(style.thickness + outline_px * 2.0).build();
                draw_list.add_line([center[0], center[1] - style.gap - style.size], [center[0], center[1] - style.gap], outline_color).thickness(style.thickness + outline_px * 2.0).build();
                draw_list.add_line([center[0], center[1] + style.gap], [center[0], center[1] + style.gap + style.size], outline_color).thickness(style.thickness + outline_px * 2.0).build();
                if style.dot { draw_list.add_rect([center[0] - style.thickness / 2.0 - outline_px, center[1] - style.thickness / 2.0 - outline_px], [center[0] + style.thickness / 2.0 + outline_px, center[1] + style.thickness / 2.0 + outline_px], outline_color).filled(true).build(); }
            }
            
            draw_list.add_line([center[0] - style.gap - style.size, center[1]], [center[0] - style.gap, center[1]], color).thickness(style.thickness).build();
            draw_list.add_line([center[0] + style.gap, center[1]], [center[0] + style.gap + style.size, center[1]], color).thickness(style.thickness).build();
            draw_list.add_line([center[0], center[1] - style.gap - style.size], [center[0], center[1] - style.gap], color).thickness(style.thickness).build();
            draw_list.add_line([center[0], center[1] + style.gap], [center[0], center[1] + style.gap + style.size], color).thickness(style.thickness).build();
            if style.dot { draw_list.add_rect([center[0] - style.thickness / 2.0, center[1] - style.thickness / 2.0], [center[0] + style.thickness / 2.0, center[1] + style.thickness / 2.0], color).filled(true).build(); }
        }

        Ok(())
    }

    fn render_debug_window(&mut self, _states: &StateRegistry, _ui: &Ui, _unicode_text: &UnicodeTextRenderer,) -> anyhow::Result<()> { Ok(()) }
}