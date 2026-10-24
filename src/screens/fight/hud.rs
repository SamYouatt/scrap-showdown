//! Fight HUD: both bots' health, part pips and boost, the clock, the
//! immobilised warning, the hint bar, and big banners ("3", "KNOCKOUT!").
use super::{Battle, Phase};
use crate::robot::Boost;
use crate::screens::Screen;
use crate::theme::prelude::*;
use bevy::prelude::*;
use bevy::time::common_conditions::on_real_timer;
use std::time::Duration;

pub(super) fn plugin(app: &mut App) {
    app.add_message::<Banner>();
    app.add_systems(
        Update,
        (
            (show_banner, fade_banners).chain(),
            (update_core_text, update_clock, update_immobilised, update_bars, update_pips).run_if(on_real_timer(Duration::from_millis(100))),
        )
            .run_if(in_state(Screen::Fight).and_then(resource_exists::<Battle>)),
    );
}

/// A big headline across the arena; replaces any banner already showing.
#[derive(Message, Clone)]
pub struct Banner {
    pub title: String,
    pub sub: String,
}

#[derive(Component)]
struct BannerNode(f32);
#[derive(Component)]
struct HpFill(usize);
#[derive(Component)]
struct CoreText(usize);
#[derive(Component)]
struct Pip {
    team: usize,
    part: usize,
}
#[derive(Component)]
struct BoostFill(usize);
#[derive(Component)]
struct ClockText;
#[derive(Component)]
struct ImmobText;

pub(super) fn spawn_hud(commands: &mut Commands, fonts: &Fonts, b: &Battle, scope: DespawnOnExit<Screen>) {
    commands
        .spawn((widget::ui_root("HUD top", Node { top: Val::Px(14.0), justify_content: JustifyContent::Center, column_gap: Val::Px(16.0), ..default() }), scope.clone()))
        .with_children(|top| {
            for (team, col) in [(0usize, Color::srgb_u8(0xff, 0x8a, 0x5c)), (1, Color::srgb_u8(0x7f, 0xb2, 0xff))] {
                if team == 1 {
                    top.spawn((
                        widget::panel(Node { min_width: Val::Px(96.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }),
                        Pickable::IGNORE,
                        children![(widget::title_text(fonts, "2:00", 50.0, CREAM), ClockText, Pickable::IGNORE)],
                    ));
                }
                let bot = &b.bots[team];
                top.spawn((widget::panel(Node { width: Val::Px(300.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(7.0), ..default() }), Pickable::IGNORE)).with_children(|c| {
                    c.spawn((
                        Node { justify_content: JustifyContent::SpaceBetween, ..default() },
                        Pickable::IGNORE,
                        children![
                            (widget::title_text(fonts, bot.label.clone(), 24.0, col), Pickable::IGNORE),
                            (widget::text(fonts, "CORE 100%", 15.0, CREAM), CoreText(team), Pickable::IGNORE),
                        ],
                    ));
                    c.spawn((
                        Node { height: Val::Px(14.0), border_radius: BorderRadius::all(Val::Px(7.0)), ..default() },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.4)),
                        Pickable::IGNORE,
                        children![(
                            Node { width: Val::Percent(100.0), height: Val::Percent(100.0), border_radius: BorderRadius::all(Val::Px(7.0)), ..default() },
                            BackgroundColor(GREEN),
                            HpFill(team),
                            Pickable::IGNORE,
                        )],
                    ));
                    c.spawn((Node { column_gap: Val::Px(4.0), flex_wrap: FlexWrap::Wrap, row_gap: Val::Px(3.0), ..default() }, Pickable::IGNORE)).with_children(|pips| {
                        for part in 0..bot.parts.len() {
                            pips.spawn((
                                Node { width: Val::Px(18.0), height: Val::Px(9.0), border_radius: BorderRadius::all(Val::Px(3.0)), ..default() },
                                BackgroundColor(Color::srgb_u8(0x5a, 0x58, 0x66)),
                                Pip { team, part },
                                Pickable::IGNORE,
                            ));
                        }
                    });
                    c.spawn((
                        Node { height: Val::Px(4.0), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
                        Pickable::IGNORE,
                        children![(
                            Node { width: Val::Percent(100.0), height: Val::Percent(100.0), border_radius: BorderRadius::all(Val::Px(2.0)), ..default() },
                            BackgroundColor(TEAL),
                            BoostFill(team),
                            Pickable::IGNORE,
                        )],
                    ));
                });
            }
        });
    commands.spawn((
        widget::ui_root("Immobilised warning", Node { top: Val::Percent(58.0), justify_content: JustifyContent::Center, ..default() }),
        scope.clone(),
        children![(widget::headline(fonts, "", 48.0, YELLOW, 4.0, 0.5), ImmobText, Pickable::IGNORE)],
    ));
    let hint = if b.test {
        "TEST DRIVE · WASD walk · CLICK weapon 1 · RIGHT-CLICK weapon 2 · SPACE handbrake · SHIFT boost · R reset · ESC back to workshop"
    } else {
        "WASD walk · CLICK / RIGHT-CLICK weapons · SPACE handbrake · SHIFT boost · hit their side or back for extra damage"
    };
    commands.spawn((
        widget::ui_root("Hint", Node { bottom: Val::Px(14.0), justify_content: JustifyContent::Center, ..default() }),
        scope,
        children![(
            widget::panel(Node { padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)), ..default() }),
            Pickable::IGNORE,
            children![(widget::text(fonts, hint, 16.0, CREAM), Pickable::IGNORE)],
        )],
    ));
}

fn show_banner(mut commands: Commands, mut msgs: MessageReader<Banner>, fonts: Res<Fonts>, old: Query<Entity, With<BannerNode>>) {
    let Some(Banner { title, sub }) = msgs.read().last().cloned() else { return };
    for e in &old {
        commands.entity(e).despawn();
    }
    let life = if title.chars().count() < 4 { 0.75 } else { 2.6 };
    commands
        .spawn((
            widget::ui_root("Banner", Node { top: Val::Percent(26.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() }),
            BannerNode(life),
            GlobalZIndex(20),
            DespawnOnExit(Screen::Fight),
        ))
        .with_children(|c| {
            c.spawn((widget::headline(&fonts, title, 104.0, YELLOW, 7.0, 0.5), Pickable::IGNORE));
            if !sub.is_empty() {
                c.spawn((widget::headline(&fonts, sub, 28.0, CREAM, 3.0, 0.5), Pickable::IGNORE));
            }
        });
}

fn fade_banners(mut commands: Commands, time: Res<Time<Real>>, mut q: Query<(Entity, &mut BannerNode)>) {
    for (e, mut b) in &mut q {
        b.0 -= time.delta_secs();
        if b.0 <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

fn update_core_text(b: Res<Battle>, mut q: Query<(&mut Text, &CoreText)>) {
    for (mut t, c) in &mut q {
        t.0 = format!("CORE {}%", (b.bots[c.0].hp_frac() * 100.0).ceil());
    }
}

fn update_clock(b: Res<Battle>, clock: Single<(&mut Text, &mut TextColor), With<ClockText>>) {
    let (mut t, mut col) = clock.into_inner();
    if b.test {
        t.0 = "∞".into();
    } else {
        let tl = b.time_left.max(0.0).ceil() as i32;
        t.0 = format!("{}:{:02}", tl / 60, tl % 60);
        col.0 = if tl <= 10 { RED } else { CREAM };
    }
}

fn update_immobilised(b: Res<Battle>, mut text: Single<&mut Text, With<ImmobText>>) {
    let mut msg = String::new();
    for k in 0..2 {
        if b.immob[k] > 3.0 && b.phase == Phase::Fight {
            msg = format!("{} immobilised… {}", b.bots[k].label, (10.0 - b.immob[k]).ceil());
        }
    }
    text.0 = msg;
}

fn update_bars(b: Res<Battle>, mut hp: Query<(&mut Node, &mut BackgroundColor, &HpFill), Without<BoostFill>>, mut boost: Query<(&mut Node, &mut BackgroundColor, &BoostFill), Without<HpFill>>) {
    for (mut n, mut bg, h) in &mut hp {
        let r = &b.bots[h.0];
        let (mut tot, mut cur) = (0.0, 0.0);
        for p in &r.parts {
            tot += p.max_hp;
            if p.attached {
                cur += p.hp;
            }
        }
        let f = cur / tot;
        n.width = Val::Percent(f * 100.0);
        bg.0 = if f < 0.3 { RED } else if f < 0.6 { YELLOW } else { GREEN };
    }
    for (mut n, mut bg, bo) in &mut boost {
        let r = &b.bots[bo.0];
        n.width = Val::Percent((1.0 - r.boost_cd / Boost::COOLDOWN) * 100.0);
        bg.0 = if r.boost_cd <= 0.0 { TEAL } else { Color::srgb_u8(0x5a, 0x5f, 0x6a) };
    }
}

fn update_pips(b: Res<Battle>, mut q: Query<(&mut BackgroundColor, &Pip)>) {
    for (mut bg, pip) in &mut q {
        let Some(part) = b.bots[pip.team].parts.get(pip.part) else { continue };
        let f = part.hp / part.max_hp;
        bg.0 = if !part.attached {
            Color::srgba(1.0, 1.0, 1.0, 0.06)
        } else if f < 0.3 {
            RED
        } else if f < 0.7 {
            YELLOW
        } else {
            Color::srgb_u8(0x5a, 0x58, 0x66)
        };
    }
}
