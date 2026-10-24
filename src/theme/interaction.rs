//! Button look and feel on top of Bevy's headless `Button` widget: hover
//! lift, press dip, faded when `InteractionDisabled`, a click on `Activate`.
use super::palette::*;
use crate::audio::{Sfx, Snd};
use bevy::picking::hover::{HoverMap, Hovered};
use bevy::prelude::*;
use bevy::ui::{InteractionDisabled, Pressed};
use bevy::ui_widgets::Activate;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<PointerOverUi>();
    app.add_systems(Update, (button_visuals, pointer_over_ui));
    app.add_observer(click_sound);
}

/// Colour scheme of a button. Required by every button the theme spawns.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[require(bevy::ui_widgets::Button, Hovered)]
pub enum BtnStyle {
    Primary,
    Secondary,
    Teal,
    /// flat swatch of any colour
    Custom(Color),
}

impl BtnStyle {
    /// (face, shadow, label)
    pub fn colours(self) -> (Color, Color, Color) {
        match self {
            BtnStyle::Primary => (YELLOW, YELLOW_DARK, INK),
            BtnStyle::Secondary => (SECONDARY, SECONDARY_DARK, CREAM),
            BtnStyle::Teal => (TEAL, Color::srgb(0.17, 0.48, 0.47), INK),
            BtnStyle::Custom(c) => (c, c.darker(0.25), INK),
        }
    }
}

fn button_visuals(mut q: Query<(&BtnStyle, &Hovered, Has<Pressed>, Has<InteractionDisabled>, &mut BackgroundColor, &mut Node)>) {
    for (style, hovered, pressed, disabled, mut bg, mut node) in &mut q {
        let base = style.colours().0;
        let (colour, top) = if disabled {
            (base.with_alpha(0.4), 0.0)
        } else if pressed {
            (base.lighter(0.06), 3.0)
        } else if hovered.0 {
            (base.lighter(0.06), -1.0)
        } else {
            (base, 0.0)
        };
        bg.set_if_neq(BackgroundColor(colour));
        if node.top != Val::Px(top) {
            node.top = Val::Px(top);
        }
    }
}

fn click_sound(_: On<Activate>, mut sfx: MessageWriter<Sfx>) {
    sfx.write(Snd::Click.play());
}

/// True while the pointer is over any UI node (so 3D scenes can ignore clicks).
#[derive(Resource, Default)]
pub struct PointerOverUi(pub bool);

fn pointer_over_ui(hover: Res<HoverMap>, nodes: Query<(), With<Node>>, mut over: ResMut<PointerOverUi>) {
    over.0 = hover.values().any(|m| m.keys().any(|e| nodes.contains(*e)));
}
