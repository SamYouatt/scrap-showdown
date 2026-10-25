//! The game's screens. Each is a [`Screen`] state with its own plugin that
//! spawns on `OnEnter`, tags what it spawns with `DespawnOnExit`, and gates
//! its `Update` systems on `in_state`.
mod fight;
mod lookdev;
mod results;
mod scrap;
mod title;
mod workshop;

use crate::params::param;
use bevy::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.init_state::<Screen>();
    app.add_plugins((title::plugin, workshop::plugin, fight::plugin, results::plugin, scrap::plugin, lookdev::plugin));
    app.add_systems(Startup, jump_to_screen);
}

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Screen {
    #[default]
    Title,
    Workshop,
    Fight,
    Results,
    Scrap,
    Lookdev,
}

/// `?scene=workshop|fight|scrap|lookdev` skips straight to a screen.
fn jump_to_screen(mut next: ResMut<NextState<Screen>>) {
    match param("scene").as_deref() {
        Some("workshop") => next.set(Screen::Workshop),
        Some("fight") => next.set(Screen::Fight),
        Some("scrap") => next.set(Screen::Scrap),
        Some("lookdev") => next.set(Screen::Lookdev),
        _ => {}
    }
}
