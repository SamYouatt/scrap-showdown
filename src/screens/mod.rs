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
}

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Screen {
    /// waiting for the models (see art.rs)
    #[default]
    Loading,
    Title,
    Workshop,
    Fight,
    Results,
    Scrap,
    Lookdev,
}

/// The screen after loading: the title, or `?scene=workshop|fight|scrap|lookdev`.
pub fn first_screen() -> Screen {
    match param("scene").as_deref() {
        Some("workshop") => Screen::Workshop,
        Some("fight") => Screen::Fight,
        Some("scrap") => Screen::Scrap,
        Some("lookdev") => Screen::Lookdev,
        _ => Screen::Title,
    }
}
