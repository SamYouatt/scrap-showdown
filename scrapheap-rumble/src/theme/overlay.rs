//! Transient overlays any screen can request with a message: toasts stacked
//! top-centre, comic-book pop words at a world position, and a white flash.
use super::palette::*;
use super::widget;
use super::Fonts;
use crate::screens::Screen;
use bevy::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.add_message::<Toast>().add_message::<PopWord>().add_message::<Flash>();
    app.add_systems(Update, ((spawn_toasts, spawn_pop_words, spawn_flash), (tick_toasts, animate_pop_words, fade_flash)).chain());
}

/// A short message stacked top-centre.
#[derive(Message, Clone)]
pub struct Toast(pub String, pub Color);

/// A comic-book word that pops up at a world position, floats up and fades.
#[derive(Message, Clone)]
pub struct PopWord {
    pub at: Vec3,
    pub big: bool,
    pub word: String,
    pub color: Color,
}

/// White full-screen flash that fades out (big impacts).
#[derive(Message, Clone, Copy)]
pub struct Flash;

#[derive(Component)]
struct ToastNode(f32);
#[derive(Component)]
struct ToastStack;

fn spawn_toasts(mut commands: Commands, mut msgs: MessageReader<Toast>, fonts: Res<Fonts>, stack: Query<Entity, With<ToastStack>>) {
    let mut stack_e = stack.single().ok();
    for Toast(msg, col) in msgs.read() {
        let stack = *stack_e.get_or_insert_with(|| {
            commands
                .spawn((
                    Name::new("Toasts"),
                    ToastStack,
                    Node { position_type: PositionType::Absolute, top: Val::Px(90.0), width: Val::Percent(100.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(6.0), ..default() },
                    Pickable::IGNORE,
                    GlobalZIndex(50),
                ))
                .id()
        });
        commands.spawn((
            ToastNode(2.5),
            widget::panel(Node::default()),
            // panel() already has a BorderColor: replace it rather than bundle a duplicate
            Pickable::IGNORE,
            ChildOf(stack),
            children![(widget::text(&fonts, msg.clone(), 17.0, CREAM), Pickable::IGNORE)],
        )).insert(BorderColor::all(*col));
    }
}

fn tick_toasts(mut commands: Commands, time: Res<Time<Real>>, mut q: Query<(Entity, &mut ToastNode)>) {
    for (e, mut t) in &mut q {
        t.0 -= time.delta_secs();
        if t.0 <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

#[derive(Component)]
struct PopWordNode {
    t: f32,
    at: Vec2,
}

/// Projects through the screen's 3D camera, so pop words only show on
/// screens that have one.
fn spawn_pop_words(
    mut commands: Commands,
    mut msgs: MessageReader<PopWord>,
    fonts: Res<Fonts>,
    screen: Res<State<Screen>>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) {
    let Ok((camera, gt)) = cam.single() else {
        msgs.clear();
        return;
    };
    for p in msgs.read() {
        let Ok(px) = camera.world_to_viewport(gt, p.at) else { continue };
        commands.spawn((
            Name::new("Pop word"),
            Node { position_type: PositionType::Absolute, left: Val::Px(px.x - 80.0), top: Val::Px(px.y - 30.0), width: Val::Px(160.0), justify_content: JustifyContent::Center, ..default() },
            PopWordNode { t: 0.0, at: px },
            Pickable::IGNORE,
            GlobalZIndex(30),
            DespawnOnExit(*screen.get()),
            children![(
                widget::title_text(&fonts, p.word.clone(), if p.big { 46.0 } else { 24.0 }, p.color),
                TextShadow { offset: Vec2::new(0.0, 4.0), color: Color::srgba(0.13, 0.12, 0.16, 0.9) },
                TextLayout::default().with_no_wrap(),
                Pickable::IGNORE,
            )],
        ));
    }
}

fn animate_pop_words(mut commands: Commands, time: Res<Time<Real>>, mut q: Query<(Entity, &mut PopWordNode, &mut Node, &Children)>, mut tc: Query<(&mut TextColor, &mut UiTransform)>) {
    for (e, mut p, mut n, ch) in &mut q {
        p.t += time.delta_secs();
        let k = p.t / 0.95;
        if k >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        n.top = Val::Px(p.at.y - 30.0 - 46.0 * k.powf(0.6));
        for c in ch.iter() {
            if let Ok((mut col, mut t)) = tc.get_mut(c) {
                let pop = if k < 0.12 { 0.4 + k / 0.12 * 0.8 } else { 1.2 - (k - 0.12).min(0.2) };
                t.scale = Vec2::splat(pop);
                t.rotation = Rot2::radians(-0.08);
                col.0 = col.0.with_alpha(if k > 0.7 { 1.0 - (k - 0.7) / 0.3 } else { 1.0 });
            }
        }
    }
}

#[derive(Component)]
struct FlashNode(f32);

fn spawn_flash(mut commands: Commands, mut msgs: MessageReader<Flash>, screen: Res<State<Screen>>) {
    if msgs.read().count() == 0 {
        return;
    }
    commands.spawn((
        Name::new("Flash"),
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8)),
        FlashNode(0.3),
        Pickable::IGNORE,
        GlobalZIndex(40),
        DespawnOnExit(*screen.get()),
    ));
}

fn fade_flash(mut commands: Commands, time: Res<Time<Real>>, mut q: Query<(Entity, &mut FlashNode, &mut BackgroundColor)>) {
    for (e, mut f, mut bg) in &mut q {
        f.0 -= time.delta_secs();
        if f.0 <= 0.0 {
            commands.entity(e).despawn();
        } else {
            bg.0 = Color::srgba(1.0, 1.0, 1.0, f.0 / 0.3 * 0.8);
        }
    }
}
