//! Shared bevy_ui look: chunky dark panels with a light edge and drop shadow,
//! Lilita for headings/buttons, Nunito for body text. Mirrors the prototype's
//! style.css so every screen looks the same.
pub mod interaction;
pub mod overlay;
pub mod palette;
pub mod widget;

use bevy::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Fonts>();
    app.add_plugins((interaction::plugin, overlay::plugin));
}

/// Fonts are loaded once at startup and shared by every screen.
#[derive(Resource, Clone)]
pub struct Fonts {
    pub title: Handle<Font>,
    pub body: Handle<Font>,
}

impl FromWorld for Fonts {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>();
        Fonts { title: assets.load("fonts/lilita.ttf"), body: assets.load("fonts/nunito-bold.ttf") }
    }
}

pub mod prelude {
    pub use super::interaction::{BtnStyle, PointerOverUi};
    pub use super::overlay::{Flash, PopWord, Toast};
    pub use super::palette::*;
    pub use super::widget;
    pub use super::Fonts;
    pub use bevy::ui::InteractionDisabled;
    pub use bevy::ui_widgets::Activate;
}
