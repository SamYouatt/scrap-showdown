//! Scrapheap Rumble — Bevy port.
//! Flow: title -> workshop -> fight -> results -> scrapheap -> workshop ...
//!
//! Layout follows the `bevy_new_2d` template: every module exposes a
//! `plugin(app)` function, screens live in `screens/` and are state-scoped,
//! shared UI widgets and overlays live in `theme/`.
mod ai;
mod arena;
mod audio;
mod fx;
mod models;
mod params;
mod partmodels;
mod parts;
mod physics;
mod preview;
mod rng;
mod robot;
mod scene;
mod screens;
mod state;
mod theme;

use bevy::asset::AssetMetaCheck;
use bevy::prelude::*;

fn main() -> AppExit {
    App::new().add_plugins(AppPlugin).run()
}

pub struct AppPlugin;

impl Plugin for AppPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    // there are no .meta files; don't make the web build 404 for each asset
                    meta_check: AssetMetaCheck::Never,
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Scrapheap Rumble".into(),
                        canvas: Some("#view".into()),
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: true,
                        ..default()
                    }),
                    ..default()
                }),
        );
        app.insert_resource(ClearColor(Color::srgb_u8(0x8f, 0xc8, 0xef)));
        app.add_plugins((audio::plugin, fx::plugin, models::plugin, scene::plugin, state::plugin, theme::plugin, screens::plugin));
    }
}
