//! Bundle functions for the common widgets.
use super::interaction::BtnStyle;
use super::palette::*;
use super::Fonts;
use bevy::prelude::*;

pub fn text(fonts: &Fonts, s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(s), TextFont { font: fonts.body.clone().into(), font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

pub fn title_text(fonts: &Fonts, s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(s), TextFont { font: fonts.title.clone().into(), font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

/// Big headline text with a drop shadow.
pub fn headline(fonts: &Fonts, s: impl Into<String>, size: f32, color: Color, shadow: f32, alpha: f32) -> impl Bundle {
    (title_text(fonts, s, size, color), TextShadow { offset: Vec2::new(0.0, shadow), color: Color::srgba(0.0, 0.0, 0.0, alpha) })
}

/// A full-window UI root that doesn't block picking.
pub fn ui_root(name: &'static str, node: Node) -> impl Bundle {
    (Name::new(name), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), ..node }, Pickable::IGNORE)
}

/// A dark rounded panel node.
pub fn panel(node: Node) -> impl Bundle {
    (
        Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(12.0)), border: UiRect::all(Val::Px(3.0)), border_radius: BorderRadius::all(Val::Px(14.0)), ..node },
        BackgroundColor(PANEL),
        BorderColor::all(PANEL_EDGE),
        BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.35), Val::Px(0.0), Val::Px(6.0), Val::Px(0.0), Val::Px(0.0)),
    )
}

/// A chunky button with a text label. Attach its action with `.observe`
/// (an `On<Activate>` observer) or an action component.
pub fn button(fonts: &Fonts, label: impl Into<String>, style: BtnStyle, size: f32) -> impl Bundle {
    let (bg, shadow, fg) = style.colours();
    (
        Name::new("Button"),
        style,
        Node {
            padding: UiRect::axes(Val::Px(size * 0.8), Val::Px(size * 0.45)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(Val::Px(10.0)),
            ..default()
        },
        BackgroundColor(bg),
        BoxShadow::new(shadow, Val::Px(0.0), Val::Px(4.0), Val::Px(0.0), Val::Px(0.0)),
        children![(title_text(fonts, label, size, fg), Pickable::IGNORE)],
    )
}

/// A flat clickable node with custom content (list rows, chips, swatches).
pub fn clickable(style: BtnStyle, node: Node) -> impl Bundle {
    (style, Node { border_radius: BorderRadius::all(Val::Px(7.0)), align_items: AlignItems::Center, ..node }, BackgroundColor(style.colours().0))
}

/// Ten-pip stat bar.
pub fn bars(v: f32, color: Color) -> impl Bundle {
    let pip = move |i: usize| {
        let on = (i as f32) < v.round();
        (
            Node { width: Val::Px(14.0), height: Val::Px(12.0), border_radius: BorderRadius::all(Val::Px(3.0)), ..default() },
            BackgroundColor(if on { color } else { Color::srgba(1.0, 1.0, 1.0, 0.08) }),
        )
    };
    (Node { column_gap: Val::Px(3.0), ..default() }, Children::spawn(SpawnIter((0..10).map(pip))))
}
