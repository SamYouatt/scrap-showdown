//! Scrapheap Rumble — Bevy port.
//! Flow: title -> workshop -> fight -> results -> scrapheap -> workshop ...
mod ai;
mod arena;
mod audio;
mod battle;
mod fx;
mod physics;
mod robot;
mod models;
mod partmodels;
mod parts;
mod preview;
mod results;
mod rng;
mod scene;
mod scrap;
mod state;
mod title;
mod ui;
mod workshop;

use bevy::prelude::*;

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Screen {
    #[default]
    Title,
    Workshop,
    Fight,
    Results,
    Scrap,
}

/// Everything spawned for the current screen; despawned on exit.
#[derive(Component, Clone, Copy, Default)]
pub struct Scoped;

fn clear_scoped(mut commands: Commands, q: Query<Entity, With<Scoped>>) {
    for e in q.iter() {
        commands.entity(e).try_despawn();
    }
}

/// `?scene=fight&opp=2` style URL params (web) or SCRAP_* env vars (native).
pub fn param(key: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let search = web_sys::window()?.location().search().ok()?;
        for kv in search.trim_start_matches('?').split('&') {
            let mut it = kv.splitn(2, '=');
            if it.next() == Some(key) {
                return Some(it.next().unwrap_or("").to_string());
            }
        }
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var(format!("SCRAP_{}", key.to_uppercase())).ok()
    }
}

fn start_screen(mut next: ResMut<NextState<Screen>>) {
    match param("scene").as_deref() {
        Some("workshop") => next.set(Screen::Workshop),
        Some("fight") => next.set(Screen::Fight),
        Some("scrap") => next.set(Screen::Scrap),
        _ => {}
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Scrapheap Rumble".into(),
            canvas: Some("#view".into()),
            fit_canvas_to_parent: true,
            prevent_default_event_handling: true,
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::srgb_u8(0x8f, 0xc8, 0xef)))
    .insert_resource(state::GameState::new_game())
    .init_state::<Screen>()
    .add_plugins((ui::UiPlugin, audio::AudioPlugin, scene::ScenePlugin, title::TitlePlugin, battle::BattlePlugin, results::ResultsPlugin, workshop::WorkshopPlugin, scrap::ScrapPlugin))
    .add_systems(Startup, start_screen);
    for s in [Screen::Title, Screen::Workshop, Screen::Fight, Screen::Results, Screen::Scrap] {
        app.add_systems(OnExit(s), clear_scoped);
    }
    app.run();
}
