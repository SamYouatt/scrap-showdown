//! Post-fight card: what happened, what you lost or salvaged, and how much
//! scrapheap time you earned.
use super::Screen;
use crate::audio::{Sfx, Snd};
use crate::models::{self, Mats};
use crate::parts::PartKind;
use crate::preview::{spawn_preview, PreviewRoot};
use crate::scene::{self, SunOpts};
use crate::state::{self, Run};
use crate::theme::prelude::*;
use bevy::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Results), spawn_results);
    app.add_systems(Update, spin_bot.run_if(in_state(Screen::Results)));
}

#[derive(Component)]
struct Spin;

fn names(kinds: &[PartKind]) -> String {
    kinds.iter().map(|k| k.def().name).collect::<Vec<_>>().join(", ")
}

fn row(c: &mut ChildSpawnerCommands, fonts: &Fonts, k: &str, v: String, big: bool) {
    c.spawn((
        Node { justify_content: JustifyContent::SpaceBetween, width: Val::Percent(100.0), ..default() },
        children![widget::text(fonts, k, 17.0, CREAM), widget::title_text(fonts, v, if big { 26.0 } else { 19.0 }, YELLOW)],
    ));
}

#[allow(clippy::too_many_arguments)]
fn spawn_results(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    shared: Res<Mats>,
    art: Res<crate::art::Art>,
    eyes: Res<crate::robot::EyeMats>,
    fonts: Res<Fonts>,
    mut s: ResMut<Run>,
    mut next: ResMut<NextState<Screen>>,
    mut sfx: MessageWriter<Sfx>,
) {
    let Some(r) = s.last.take() else {
        next.set(Screen::Workshop);
        return;
    };
    let scope = DespawnOnExit(Screen::Results);
    let opps = state::opponents();
    s.results.push(r.won);
    let lost: Vec<_> = r.lost.iter().filter_map(|id| s.design.parts.iter().find(|p| p.id == *id)).map(|p| p.kind).filter(|k| *k != PartKind::Core).collect();
    let salvaged = s.apply_battle(&r);
    if !r.won {
        s.lives -= 1;
    }
    let champion = r.won && s.round == opps.len() - 1;
    let game_over = !r.won && s.lives <= 0;
    if r.won {
        s.round = (s.round + 1).min(opps.len() - 1);
    }
    let (total, lines) = state::scrap_time_for(&r);
    s.scrap_time = total;

    // backdrop: your (now battered) bot turning on a plinth
    models::reseed(5);
    scene::sun_lights(&mut commands, SunOpts { size: 5.0, dir: Vec3::new(4.0, 8.0, -6.0), ..default() }, scope.clone());
    scene::sky_and_clouds(&mut commands, &mut meshes, &mut mats, Color::srgb_u8(0x3f, 0x93, 0xe6), Color::srgb_u8(0xb8, 0xdc, 0xf6), 8, 30.0, scope.clone());
    let ground = models::cyl(3.2, 3.2, 0.4, 10, models::hexc("#c79a62"), 0.0);
    commands.spawn((Name::new("Plinth"), Mesh3d(meshes.add(ground.to_mesh())), MeshMaterial3d(shared.vc.clone()), Transform::from_xyz(-2.6, -0.2, 0.0), scope.clone()));
    let e = spawn_preview(&mut commands, &mut meshes, &art, &eyes, &s.design, false, Transform::from_xyz(-2.6, 0.0, 0.0), scope.clone());
    commands.entity(e).insert(Spin);
    commands.spawn((scene::camera_bundle(36.0, Transform::from_xyz(0.0, 2.8, -8.0).looking_at(Vec3::new(0.0, 0.8, 0.0), Vec3::Y)), scope.clone()));

    let ladder: Vec<(String, i32)> = opps.iter().enumerate().map(|(i, o)| (o.design.name.clone(), (i as i32 - s.round as i32).signum())).collect();
    let fonts = &*fonts;
    let root = Node { height: Val::Percent(100.0), align_items: AlignItems::Center, padding: UiRect::left(Val::Vw(8.0)), ..default() };
    commands.spawn((widget::ui_root("Results UI", root), scope)).with_children(|p| {
        p.spawn(widget::panel(Node { width: Val::Px(520.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(10.0), padding: UiRect::all(Val::Px(22.0)), ..default() })).with_children(|c| {
            c.spawn(widget::headline(fonts, if r.won { "VICTORY!" } else { "DEFEAT" }, 64.0, if r.won { YELLOW } else { RED }, 4.0, 0.5));
            c.spawn(widget::title_text(fonts, r.how.clone(), 24.0, CREAM));
            c.spawn(Node { column_gap: Val::Px(6.0), flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, ..default() }).with_children(|l| {
                for (name, st) in &ladder {
                    let (bg, fg) = match st {
                        -1 => (Color::srgba(0.486, 0.812, 0.353, 0.25), GREEN),
                        0 => (YELLOW, Color::srgb_u8(0x22, 0x1f, 0x29)),
                        _ => (Color::srgba(1.0, 1.0, 1.0, 0.06), Color::srgba(1.0, 0.95, 0.86, 0.5)),
                    };
                    l.spawn((
                        Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)), border_radius: BorderRadius::all(Val::Px(6.0)), ..default() },
                        BackgroundColor(bg),
                        children![widget::text(fonts, name.clone(), 13.0, fg)],
                    ));
                }
            });
            row(c, fonts, "Damage dealt", format!("{}", r.dmg_dealt), false);
            row(c, fonts, "Damage taken", format!("{}", r.dmg_taken), false);
            row(c, fonts, "Parts knocked off", format!("{}", lost.len()), false);
            if !lost.is_empty() {
                let tail = if r.won {
                    if salvaged.is_empty() { "Nothing worth salvaging.".to_string() } else { format!("Salvaged from the arena floor: {}", names(&salvaged)) }
                } else {
                    "All left behind in the arena.".into()
                };
                c.spawn((widget::text(fonts, format!("Knocked off: {}.\n{}", names(&lost), tail), 14.0, CREAM), TextLayout::default().with_justify(Justify::Center)));
            }
            if let Some(t) = r.trophy {
                c.spawn((widget::text(fonts, format!("Trophy: you claim {} from {}'s wreck!", t.def().name, r.opponent), 16.0, YELLOW), TextLayout::default().with_justify(Justify::Center)));
            }
            if champion || game_over {
                let msg = if champion { "You are the Scrapheap Champion!" } else { "Out of lives. Your bot is going back on the heap." };
                c.spawn(widget::title_text(fonts, msg, 22.0, CREAM));
                c.spawn(widget::button(fonts, "New run", BtnStyle::Primary, 26.0)).observe(new_run);
            } else {
                c.spawn((
                    Node { flex_direction: FlexDirection::Column, width: Val::Percent(100.0), row_gap: Val::Px(4.0), padding: UiRect::all(Val::Px(12.0)), border_radius: BorderRadius::all(Val::Px(10.0)), ..default() },
                    BackgroundColor(Color::srgb_u8(0x2a, 0x28, 0x32)),
                ))
                .with_children(|t| {
                    t.spawn(widget::title_text(fonts, "SCRAPHEAP TIME", 16.0, TEAL));
                    for (k, v) in &lines {
                        row(t, fonts, k, format!("+{v}s"), false);
                    }
                    row(t, fonts, "Total", format!("{total}s"), true);
                    if !r.won {
                        t.spawn(widget::text(fonts, format!("Lives left: {} · you'll rematch {}", s.lives, r.opponent), 14.0, CREAM));
                    }
                });
                c.spawn(widget::button(fonts, "To the scrapheap!", BtnStyle::Primary, 28.0)).observe(|_: On<Activate>, mut next: ResMut<NextState<Screen>>| next.set(Screen::Scrap));
            }
        });
    });
    sfx.write(Snd::Ding.pitched(if r.won { 1.2 } else { 0.7 }));
}

fn new_run(_: On<Activate>, mut run: ResMut<Run>, mut next: ResMut<NextState<Screen>>) {
    *run = Run::new_game();
    next.set(Screen::Title);
}

fn spin_bot(time: Res<Time>, mut q: Query<(&mut Transform, &PreviewRoot), With<Spin>>) {
    for (mut tf, r) in &mut q {
        tf.rotation = Quat::from_rotation_y(std::f32::consts::PI + time.elapsed_secs() * 0.5);
        tf.translation.y = r.ride;
    }
}
