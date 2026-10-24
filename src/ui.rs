//! Shared bevy_ui look: chunky dark panels with a light edge and drop shadow,
//! Lilita for headings/buttons, Nunito for body text. Mirrors the prototype's
//! style.css so every screen looks the same.
use bevy::prelude::*;

pub const INK: Color = Color::srgb(0.137, 0.129, 0.165); // #23212a
pub const PANEL: Color = Color::srgba(0.125, 0.118, 0.157, 0.92); // rgba(32,30,40,.92)
pub const PANEL_EDGE: Color = Color::srgb(0.29, 0.271, 0.333); // #4a4555
pub const YELLOW: Color = Color::srgb(0.965, 0.718, 0.235); // #f6b73c
pub const YELLOW_DARK: Color = Color::srgb(0.663, 0.435, 0.071); // #a96f12
pub const ORANGE: Color = Color::srgb(0.933, 0.478, 0.196);
pub const RED: Color = Color::srgb(0.886, 0.282, 0.239); // #e2483d
pub const TEAL: Color = Color::srgb(0.298, 0.765, 0.757); // #4cc3c1
pub const CREAM: Color = Color::srgb(1.0, 0.953, 0.863); // #fff3dc
pub const GREEN: Color = Color::srgb(0.486, 0.812, 0.353); // #7ccf5a
pub const SECONDARY: Color = Color::srgb(0.357, 0.337, 0.416); // #5b566a
pub const SECONDARY_DARK: Color = Color::srgb(0.184, 0.173, 0.22);
pub const MUTED: Color = Color::srgb(0.851, 0.8, 0.706); // #d9ccb4

#[derive(Resource, Clone)]
pub struct Fonts {
    pub title: Handle<Font>,
    pub body: Handle<Font>,
    pub black: Handle<Font>,
}


/// Marker for a clickable button. `id` is matched by the screen that owns it.
#[derive(Component, Clone, Debug)]
pub struct Btn {
    pub id: String,
    pub style: BtnStyle,
    pub disabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BtnStyle {
    Primary,
    Secondary,
    Teal,
    Danger,
    /// flat swatch of any colour
    Custom(Color),
}

impl BtnStyle {
    pub fn colours(self) -> (Color, Color, Color) {
        match self {
            BtnStyle::Primary => (YELLOW, YELLOW_DARK, INK),
            BtnStyle::Secondary => (SECONDARY, SECONDARY_DARK, CREAM),
            BtnStyle::Teal => (TEAL, Color::srgb(0.17, 0.48, 0.47), INK),
            BtnStyle::Danger => (RED, Color::srgb(0.557, 0.149, 0.125), Color::WHITE),
            BtnStyle::Custom(c) => (c, c.darker(0.25), INK),
        }
    }
}

/// Fired when a `Btn` is clicked (and not disabled).
#[derive(Message, Clone, Debug)]
pub struct Clicked(pub String);

/// True while the pointer is over any UI node (so 3D scenes can ignore clicks).
#[derive(Resource, Default)]
pub struct PointerOverUi(pub bool);

pub fn text(fonts: &Fonts, s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(s), TextFont { font: fonts.body.clone().into(), font_size: FontSize::Px(size), ..default() }, TextColor(color))
}
pub fn title_text(fonts: &Fonts, s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(s), TextFont { font: fonts.title.clone().into(), font_size: FontSize::Px(size), ..default() }, TextColor(color))
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

/// Spawn a chunky button with a label as a child.
pub fn button(parent: &mut ChildSpawnerCommands, fonts: &Fonts, id: &str, label: &str, style: BtnStyle, size: f32) -> Entity {
    let (bg, shadow, fg) = style.colours();
    parent
        .spawn((
            Btn { id: id.into(), style, disabled: false },
            Node {
                padding: UiRect::axes(Val::Px(size * 0.8), Val::Px(size * 0.45)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(bg),
            BoxShadow::new(shadow, Val::Px(0.0), Val::Px(4.0), Val::Px(0.0), Val::Px(0.0)),
        ))
        .with_children(|b| {
            b.spawn((title_text(fonts, label, size, fg), Pickable::IGNORE));
        })
        .id()
}

/// Ten-pip stat bar.
pub fn bars(parent: &mut ChildSpawnerCommands, v: f32, color: Color) {
    parent
        .spawn(Node { column_gap: Val::Px(3.0), ..default() })
        .with_children(|b| {
            for i in 0..10 {
                let on = (i as f32) < v.round();
                b.spawn((
                    Node { width: Val::Px(14.0), height: Val::Px(12.0), border_radius: BorderRadius::all(Val::Px(3.0)), ..default() },
                    BackgroundColor(if on { color } else { Color::srgba(1.0, 1.0, 1.0, 0.08) }),
                ));
            }
        });
}

/// Toasts: short messages stacked top-centre.
#[derive(Message, Clone)]
pub struct Toast(pub String, pub Color);

#[derive(Component)]
struct ToastNode(f32);
#[derive(Component)]
struct ToastWrap;

fn spawn_toasts(mut commands: Commands, mut msgs: MessageReader<Toast>, fonts: Option<Res<Fonts>>, wrap: Query<Entity, With<ToastWrap>>) {
    let Some(fonts) = fonts else { return };
    for Toast(msg, col) in msgs.read() {
        let w = match wrap.single() {
            Ok(w) => w,
            Err(_) => commands
                .spawn((
                    ToastWrap,
                    Node { position_type: PositionType::Absolute, top: Val::Px(90.0), width: Val::Percent(100.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(6.0), ..default() },
                    Pickable::IGNORE,
                    GlobalZIndex(50),
                ))
                .id(),
        };
        commands.entity(w).with_children(|p| {
            p.spawn((ToastNode(2.5), panel(Node { border: UiRect::all(Val::Px(3.0)), ..default() }), BorderColor::all(*col), Pickable::IGNORE))
                .with_children(|t| {
                    t.spawn((text(&fonts, msg.clone(), 17.0, CREAM), Pickable::IGNORE));
                });
        });
    }
}

fn tick_toasts(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut ToastNode)>) {
    for (e, mut t) in q.iter_mut() {
        t.0 -= time.delta_secs();
        if t.0 <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

/// Hover lift, press dip, click message. Reads the picking pointer messages.
fn button_feedback(
    mut q: Query<(&Btn, &mut BackgroundColor, &mut Node)>,
    mut over: MessageReader<bevy::picking::events::PointerOver>,
    mut out: MessageReader<bevy::picking::events::PointerOut>,
    mut press: MessageReader<bevy::picking::events::PointerPress>,
    mut release: MessageReader<bevy::picking::events::PointerRelease>,
    mut click: MessageReader<bevy::picking::events::PointerClick>,
    mut clicked: MessageWriter<Clicked>,
) {
    use bevy::picking::pointer::PointerButton;
    for e in over.read() {
        if let Ok((b, mut bg, mut node)) = q.get_mut(e.entity) {
            if !b.disabled {
                node.top = Val::Px(-1.0);
                *bg = BackgroundColor(b.style.colours().0.lighter(0.06));
            }
        }
    }
    for e in out.read() {
        if let Ok((b, mut bg, mut node)) = q.get_mut(e.entity) {
            node.top = Val::Px(0.0);
            let base = b.style.colours().0;
            *bg = BackgroundColor(if b.disabled { base.with_alpha(0.4) } else { base });
        }
    }
    for e in press.read() {
        if let Ok((b, _, mut node)) = q.get_mut(e.entity) {
            if !b.disabled && e.button == PointerButton::Primary {
                node.top = Val::Px(3.0);
            }
        }
    }
    for e in release.read() {
        if let Ok((_, _, mut node)) = q.get_mut(e.entity) {
            node.top = Val::Px(-1.0);
        }
    }
    for e in click.read() {
        if let Ok((b, _, _)) = q.get(e.entity) {
            if !b.disabled && e.button == PointerButton::Primary {
                clicked.write(Clicked(b.id.clone()));
                crate::audio::click();
            }
        }
    }
}

/// Keep disabled buttons faded.
fn button_disabled(mut q: Query<(&Btn, &mut BackgroundColor), Changed<Btn>>) {
    for (b, mut bg) in q.iter_mut() {
        let (base, _, _) = b.style.colours();
        *bg = BackgroundColor(if b.disabled { base.with_alpha(0.4) } else { base });
    }
}

/// Whether the pointer is over any UI node (3D scenes ignore clicks then).
fn pointer_over_ui(hover: Option<Res<bevy::picking::hover::HoverMap>>, nodes: Query<(), With<Node>>, mut over: ResMut<PointerOverUi>) {
    over.0 = hover.is_some_and(|h| h.values().any(|m| m.keys().any(|e| nodes.contains(*e))));
}

/// Every UI root that belongs to a screen gets this, so the screen can clear it.
#[derive(Component)]
pub struct ScreenUi;

pub struct UiPlugin;
impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Clicked>()
            .add_message::<Toast>()
            .init_resource::<PointerOverUi>()
            .add_systems(Update, (button_feedback, button_disabled, pointer_over_ui, spawn_toasts, tick_toasts));
    }
    // inserted here rather than in a startup system: the first screen's
    // OnEnter runs before startup commands are applied
    fn finish(&self, app: &mut App) {
        let assets = app.world().resource::<AssetServer>().clone();
        app.insert_resource(Fonts {
            title: assets.load("fonts/lilita.ttf"),
            body: assets.load("fonts/nunito-bold.ttf"),
            black: assets.load("fonts/nunito-black.ttf"),
        });
        let mats = crate::models::make_mats(&mut app.world_mut().resource_mut::<Assets<StandardMaterial>>());
        app.insert_resource(mats);
    }
}
