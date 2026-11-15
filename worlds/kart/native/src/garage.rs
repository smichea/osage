// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The selection screen between races (D-131): what is raced next, who
//! drives and in what, chosen by sight rather than by name.
//!
//! - **Events** are cards showing their circuit from above — a cup its
//!   tracks side by side — under tabs: cups, races, time trials, battles.
//!   A click picks one (the host's is raced, the others' are votes).
//! - **Drivers** are portraits: each driver's kart, in their colours,
//!   filmed by a camera of its own into a picture, once, far from the
//!   circuit, in the circuit's light.
//! - **Karts** are cards with their character as bars: top speed,
//!   acceleration, handling, grip.
//! - While a circuit or its models load, a spinner turns.
//!
//! The keys still work (arrows, T); C hides or shows the screen. The
//! choices go to the world through `play` ([`Pick`]), which fills
//! [`Garage`] each frame.

use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    camera::RenderTarget,
    picking::{events::PointerClick, hover::Hovered},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};
use osage_world_kart::proto;

use crate::theme;

/// What the selection screen offers, as `play` knows it this frame.
#[derive(Resource, Default)]
pub struct Garage {
    /// Between races, with nothing else holding the keys.
    pub open: bool,
    pub drivers: Vec<proto::RosterDriver>,
    pub karts: Vec<proto::RosterKart>,
    pub events: Vec<String>,
    /// The event raced next.
    pub event: String,
    /// The driver and kart picked, as indices.
    pub chosen: (usize, usize),
    /// The event this player picked, as an index.
    pub chosen_event: Option<usize>,
    /// Whether this player's pick is raced (else it is a vote).
    pub hosting: bool,
}

/// Whether the screen shows: the HUD hides what it covers.
#[derive(Resource, Default)]
pub struct Shown(pub bool);

/// A card clicked, for `play` to send to the world.
#[derive(Resource, Default)]
pub struct Pick(pub Option<Picked>);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Picked {
    Event(usize),
    Driver(usize),
    Kart(usize),
}

pub struct GaragePlugin;

impl Plugin for GaragePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Garage>()
            .init_resource::<Pick>()
            .init_resource::<Screen>()
            .init_resource::<Shown>()
            .add_systems(Startup, setup)
            .add_systems(Update, (toggle, build_cards, into_studio, portraits, highlight, spin).chain());
    }
}

/// The kinds of event, as tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    Cup,
    Race,
    Trial,
    Battle,
}

impl Kind {
    const ALL: [Kind; 4] = [Kind::Cup, Kind::Race, Kind::Trial, Kind::Battle];

    fn of(event: &str) -> Kind {
        match event.split_once(':').map(|(kind, _)| kind) {
            Some("cup") => Kind::Cup,
            Some("trial") => Kind::Trial,
            Some("battle") => Kind::Battle,
            _ => Kind::Race,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Kind::Cup => "CUPS",
            Kind::Race => "RACES",
            Kind::Trial => "TIME TRIALS",
            Kind::Battle => "BATTLES",
        }
    }
}

/// The sections of the screen, one shown at a time.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Section {
    #[default]
    Event,
    Driver,
    Kart,
}

impl Section {
    const ALL: [Section; 3] = [Section::Event, Section::Driver, Section::Kart];

    fn title(self) -> &'static str {
        match self {
            Section::Event => "NEXT EVENT",
            Section::Driver => "DRIVER",
            Section::Kart => "KART",
        }
    }
}

/// What a tab shows: a section, or a kind of event.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum TabOf {
    Section(Section),
    Kind(Kind),
}

/// The event kinds' tabs, shown with the event section only.
#[derive(Component)]
struct KindTabs;

/// A row of cards, wrapping on a narrow window.
fn row() -> Node {
    Node { column_gap: px(8), row_gap: px(8), flex_wrap: FlexWrap::Wrap, ..default() }
}

/// A tab: a word in a frame, clicked to show what it names.
fn tab(of: TabOf, title: &str, size: f32) -> impl Bundle {
    (
        of,
        Hovered::default(),
        Node {
            padding: UiRect::axes(px(10), px(4)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BorderColor::all(theme::EDGE),
        BackgroundColor(Color::NONE),
        children![words(title, size, theme::MUTED)],
        Observer::new(|click: On<PointerClick>, tabs: Query<&TabOf>, mut screen: ResMut<Screen>| {
            match tabs.get(click.event_target()) {
                Ok(TabOf::Section(section)) => screen.section = *section,
                Ok(TabOf::Kind(kind)) => (screen.tab, screen.section) = (Some(*kind), Section::Event),
                Err(_) => {}
            }
        }),
    )
}

/// The screen's own state.
#[derive(Resource, Default)]
struct Screen {
    /// Hidden by the player (C).
    hidden: bool,
    /// The section shown.
    section: Section,
    /// What was picked a frame ago: a change by the keys shows its section.
    last: Option<((usize, usize), Option<usize>)>,
    /// Whether the screen was open a frame ago: opening films the portraits again.
    was_open: bool,
    /// The tab shown; none until the first event is known.
    tab: Option<Kind>,
    /// What the cards were built for: the tab, the events, the roster.
    built: Option<(Kind, Vec<String>, usize, usize)>,
    /// Each event's picture, made once.
    maps: HashMap<String, Handle<Image>>,
    /// Each driver colour's portrait, and its camera's frames left to film.
    portraits: HashMap<u32, (Handle<Image>, Entity, u32)>,
}

#[derive(Component)]
struct Root;
#[derive(Component)]
struct Spinner;
#[derive(Component)]
struct Ring;
#[derive(Component)]
struct Card(Picked);
/// The rows the cards are built in.
#[derive(Component)]
struct EventRow;
#[derive(Component)]
struct DriverRow;
#[derive(Component)]
struct KartRow;
/// A portrait's camera.
#[derive(Component)]
struct PortraitCamera;

/// Size of an event card's picture, of a portrait, px.
const MAP: (u32, u32) = (150, 96);
const PORTRAIT: (u32, u32) = (120, 90);
/// Where portraits are filmed: far from any circuit, each kart 30 m apart.
const STUDIO: Vec3 = Vec3::new(20_000.0, 0.0, 20_000.0);
/// The studio's own layer: only its cameras see its karts.
const STUDIO_LAYER: usize = 7;
/// The studio's exposure, EV100, and its lamps, lumens: a key lamp high in
/// front, a fill from the other side, each reaching only its own kart.
/// (Render layers do not keep a light off other meshes, only out of their
/// shadows: lamps of the whole sky, as these first were, lit the circuit
/// with a flat grey light.)
const STUDIO_EV100: f32 = 9.0;
const KEY: (f32, Vec3) = (2.0e5, Vec3::new(1.6, 2.6, 2.4));
const FILL: (f32, Vec3) = (7.0e4, Vec3::new(-2.4, 1.0, 0.8));
/// How far a studio lamp reaches, m: well short of the next kart's.
const LAMP_RANGE: f32 = 9.0;
/// Frames a portrait's camera films before it stops: long enough for its
/// pipelines to be made, its kart built and the exposure to settle.
const FILMED: u32 = 240;

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let absolute = |node: Node| Node { position_type: PositionType::Absolute, ..node };
    commands
        .spawn((
            Root,
            absolute(Node {
                bottom: px(64),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(8),
                ..default()
            }),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn(theme::panel(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                max_width: percent(96),
                ..default()
            }))
            .with_children(|panel| {
                // The sections, and the event's kinds beside them.
                panel.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(
                    |tabs| {
                        for section in Section::ALL {
                            tabs.spawn(tab(TabOf::Section(section), section.title(), 13.0));
                        }
                        tabs.spawn((Node { width: px(14), ..default() }, KindTabs));
                        for kind in Kind::ALL {
                            tabs.spawn((tab(TabOf::Kind(kind), kind.title(), 11.0), KindTabs));
                        }
                        tabs.spawn((Node { width: px(14), ..default() },));
                        tabs.spawn(words("C to hide", 10.0, theme::MUTED));
                    },
                );
                panel.spawn((EventRow, Section::Event, row()));
                panel.spawn((DriverRow, Section::Driver, row()));
                panel.spawn((KartRow, Section::Kart, row()));
            });
        });
    // The spinner, in the middle of the screen, while things load.
    let ring = images.add(ring_image(64));
    commands
        .spawn((
            Spinner,
            absolute(Node {
                top: percent(46),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(10),
                ..default()
            }),
            Visibility::Hidden,
        ))
        .with_children(|spinner| {
            spinner.spawn((Ring, ImageNode::new(ring), Node { width: px(48), height: px(48), ..default() }));
            spinner.spawn(words("LOADING", 13.0, theme::MUTED));
        });
}

fn words(text: &str, size: f32, colour: Color) -> impl Bundle {
    (Text::new(text), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(colour))
}

/// A ring, bright at one end and fading round to nothing: turned, it spins.
fn ring_image(size: u32) -> Image {
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let c = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let r = (dx * dx + dy * dy).sqrt() / c;
            let edge = (1.0 - ((r - 0.78).abs() / 0.12)).clamp(0.0, 1.0);
            let turn = (dy.atan2(dx) / std::f32::consts::TAU).rem_euclid(1.0);
            let a = (edge.min(1.0) * turn * 255.0) as u8;
            let at = ((y * size + x) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&[255, 196, 64, a]);
        }
    }
    picture(size, size, pixels)
}

fn picture(width: u32, height: u32, pixels: Vec<u8>) -> Image {
    Image::new(
        Extent3d { width, height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// The circuits of an event from above: one, or a cup's side by side.
fn event_map(event: &str) -> Image {
    let (kind, id) = event.split_once(':').unwrap_or(("track", event));
    let tracks: Vec<osage_kart_tracks::TrackDef> = match kind {
        "cup" => osage_kart_cup::built_in()
            .into_iter()
            .find(|cup| cup.id == id)
            .map(|cup| cup.tracks.iter().filter_map(|t| osage_kart_tracks::by_id(t)).collect())
            .unwrap_or_default(),
        "battle" => osage_kart_tracks::arena(id).into_iter().collect(),
        _ => osage_kart_tracks::by_id(id).into_iter().collect(),
    };
    let (w, h) = MAP;
    let mut alpha = vec![0u8; (w * h) as usize];
    let n = tracks.len().max(1);
    let (cols, rows) = if n == 1 { (1, 1) } else { (n.div_ceil(2), 2) };
    for (k, def) in tracks.iter().enumerate() {
        let circuit = osage_world_kart::Circuit::of(def);
        let points: Vec<Vec2> = circuit.track.points().iter().copied().map(Vec2::from).collect();
        let cell = Vec2::new(w as f32 / cols as f32, h as f32 / rows as f32);
        let origin = Vec2::new((k % cols) as f32 * cell.x, (k / cols) as f32 * cell.y);
        draw_circuit(&mut alpha, (w, h), &points, origin, cell);
    }
    let pixels =
        alpha.iter().flat_map(|&a| if a == 255 { [255, 255, 255, 240] } else { [255, 196, 64, a / 2] }).collect();
    picture(w, h, pixels)
}

/// A circuit's line drawn into `alpha` (a `size` picture), fitted in the
/// cell at `origin` of size `cell`, seen from above as the map shows it.
fn draw_circuit(alpha: &mut [u8], size: (u32, u32), points: &[Vec2], origin: Vec2, cell: Vec2) {
    if points.len() < 2 {
        return;
    }
    let (low, high) = points.iter().fold((Vec2::MAX, Vec2::MIN), |(l, h), p| (l.min(*p), h.max(*p)));
    let margin = 8.0;
    let scale = ((cell - 2.0 * margin) / (high - low).max(Vec2::ONE)).min_element();
    let centre = (low + high) / 2.0;
    // As the race's map: +X on the left.
    let on = |p: Vec2| origin + cell / 2.0 + Vec2::new(centre.x - p.x, centre.y - p.y) * scale;
    let mut stamp = |at: Vec2, radius: f32, strength: u8| {
        let r = radius.ceil() as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (at.x as i32 + dx, at.y as i32 + dy);
                if (dx * dx + dy * dy) as f32 <= radius * radius
                    && (0..size.0 as i32).contains(&x)
                    && (0..size.1 as i32).contains(&y)
                {
                    let pixel = &mut alpha[y as usize * size.0 as usize + x as usize];
                    *pixel = (*pixel).max(strength);
                }
            }
        }
    };
    for (radius, strength) in [(3.0, 110), (1.4, 255)] {
        for (i, a) in points.iter().enumerate() {
            let b = points[(i + 1) % points.len()];
            for step in 0..6 {
                stamp(on(a.lerp(b, step as f32 / 6.0)), radius, strength);
            }
        }
    }
}

/// An event's name on its card.
fn event_title(event: &str) -> String {
    let (kind, id) = event.split_once(':').unwrap_or(("track", event));
    let track = |id: &str| osage_kart_tracks::by_id(id).map_or_else(|| id.to_owned(), |t| t.name);
    match kind {
        "cup" => osage_kart_cup::built_in().into_iter().find(|c| c.id == id).map_or_else(|| id.to_owned(), |c| c.name),
        "battle" => osage_kart_tracks::arena(id).map_or_else(|| id.to_owned(), |a| a.name),
        _ => track(id),
    }
}

/// C hides the screen or shows it again.
fn toggle(
    keys: Res<ButtonInput<KeyCode>>,
    garage: Res<Garage>,
    mut screen: ResMut<Screen>,
    mut root: Query<&mut Visibility, With<Root>>,
    mut shown: ResMut<Shown>,
    mut sections: Query<(&Section, &mut Node)>,
    mut kind_tabs: Query<&mut Node, (With<KindTabs>, Without<Section>)>,
) {
    if keys.just_pressed(KeyCode::KeyC) && garage.open {
        screen.hidden = !screen.hidden;
    }
    // A pick made with the keys shows its section.
    let now = (garage.chosen, garage.chosen_event);
    if let Some((chosen, event)) = screen.last
        && garage.open
    {
        if chosen.0 != now.0.0 {
            screen.section = Section::Driver;
        } else if chosen.1 != now.0.1 {
            screen.section = Section::Kart;
        } else if event != now.1 {
            screen.section = Section::Event;
            screen.tab = now.1.and_then(|i| garage.events.get(i)).map(|e| Kind::of(e));
        }
    }
    screen.last = garage.open.then_some(now);
    // Opening films the portraits again, in the light of now.
    if garage.open && !screen.was_open {
        for (_, _, left) in screen.portraits.values_mut() {
            *left = FILMED;
        }
    }
    screen.was_open = garage.open;
    let open = garage.open && !screen.hidden;
    if shown.0 != open {
        shown.0 = open;
    }
    if let Ok(mut visibility) = root.single_mut() {
        visibility.set_if_neq(if open { Visibility::Inherited } else { Visibility::Hidden });
    }
    for (section, mut node) in &mut sections {
        let display = if *section == screen.section { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    for mut node in &mut kind_tabs {
        let display = if screen.section == Section::Event { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
}

/// Builds the cards again when the tab, the events or the roster change.
#[allow(clippy::too_many_arguments)]
fn build_cards(
    mut commands: Commands,
    garage: Res<Garage>,
    mut screen: ResMut<Screen>,
    mut images: ResMut<Assets<Image>>,
    events: Query<Entity, With<EventRow>>,
    drivers: Query<Entity, With<DriverRow>>,
    karts: Query<Entity, With<KartRow>>,
) {
    if !garage.open || garage.events.is_empty() && garage.drivers.is_empty() {
        return;
    }
    let tab = *screen.tab.get_or_insert_with(|| {
        garage.chosen_event.and_then(|i| garage.events.get(i)).map_or(Kind::Race, |e| Kind::of(e))
    });
    let wanted = (tab, garage.events.clone(), garage.drivers.len(), garage.karts.len());
    if screen.built.as_ref() == Some(&wanted) {
        return;
    }
    let roster_changed = screen.built.as_ref().is_none_or(|b| (b.2, b.3) != (wanted.2, wanted.3));
    screen.built = Some(wanted);
    if let Ok(row) = events.single() {
        commands.entity(row).despawn_children();
        for (i, event) in garage.events.iter().enumerate().filter(|(_, e)| Kind::of(e) == tab) {
            let map = screen.maps.entry(event.clone()).or_insert_with(|| images.add(event_map(event))).clone();
            let title = event_title(event);
            commands.entity(row).with_children(|row| {
                row.spawn(card(Picked::Event(i), 150.0)).observe(clicked(Picked::Event(i))).with_children(|card| {
                    card.spawn((ImageNode::new(map), Node { width: px(MAP.0), height: px(MAP.1), ..default() }));
                    card.spawn(words(&title, 13.0, theme::TEXT));
                });
            });
        }
    }
    if !roster_changed {
        return;
    }
    if let Ok(row) = drivers.single() {
        commands.entity(row).despawn_children();
        for (i, driver) in garage.drivers.iter().enumerate() {
            let portrait = portrait(&mut commands, &mut screen, &mut images, driver.color, i);
            let (name, class) = (driver.name.clone(), driver.class.to_uppercase());
            commands.entity(row).with_children(|row| {
                row.spawn(card(Picked::Driver(i), PORTRAIT.0 as f32))
                    .observe(clicked(Picked::Driver(i)))
                    .with_children(|card| {
                        card.spawn((
                            ImageNode::new(portrait),
                            Node { width: px(PORTRAIT.0), height: px(PORTRAIT.1), ..default() },
                        ));
                        card.spawn(words(&name, 13.0, theme::TEXT));
                        card.spawn(words(&class, 10.0, theme::MUTED));
                    });
            });
        }
    }
    if let Ok(row) = karts.single() {
        commands.entity(row).despawn_children();
        for (i, kart) in garage.karts.iter().enumerate() {
            let build = kart.build.unwrap_or_default();
            let bars =
                [("SPEED", build.top), ("ACCEL", build.accel), ("HANDLING", build.handling), ("GRIP", build.grip)];
            let name = kart.name.clone();
            commands.entity(row).with_children(|row| {
                row.spawn(card(Picked::Kart(i), 132.0)).observe(clicked(Picked::Kart(i))).with_children(|card| {
                    card.spawn(words(&name, 13.0, theme::TEXT));
                    for (label, value) in bars {
                        card.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() })
                            .with_children(|line| {
                                line.spawn((words(label, 9.0, theme::MUTED), Node { width: px(52), ..default() }));
                                // 0.85 of the standard is empty, 1.15 full.
                                let fill = (((value - 0.85) / 0.3).clamp(0.05, 1.0) * 70.0).round();
                                line.spawn((
                                    Node { width: px(70), height: px(6), ..default() },
                                    BackgroundColor(theme::EDGE),
                                ))
                                .with_children(|bar| {
                                    bar.spawn((
                                        Node { width: px(fill), height: percent(100), ..default() },
                                        BackgroundColor(theme::ACCENT),
                                    ));
                                });
                            });
                    }
                });
            });
        }
    }
}

/// A card: a button with a border that says whether it is picked.
fn card(picked: Picked, width: f32) -> impl Bundle {
    (
        Card(picked),
        Hovered::default(),
        Node {
            width: px(width + 12.0),
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            padding: UiRect::all(px(6)),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BorderColor::all(theme::EDGE),
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.03)),
    )
}

/// Every part of a portrait's kart on the studio's layer, once built.
fn into_studio(
    mut commands: Commands,
    parked: Query<Entity, With<Portrait>>,
    children: Query<&Children>,
    layered: Query<(), With<bevy::camera::visibility::RenderLayers>>,
) {
    for root in &parked {
        for part in std::iter::once(root).chain(children.iter_descendants(root)) {
            if layered.get(part).is_err() {
                commands.entity(part).insert(bevy::camera::visibility::RenderLayers::layer(STUDIO_LAYER));
            }
        }
    }
}

/// A kart parked in the studio for a portrait.
#[derive(Component)]
struct Portrait;

/// What a click on a card picks.
fn clicked(picked: Picked) -> impl FnMut(On<PointerClick>, ResMut<Pick>) {
    move |_, mut pick| pick.0 = Some(picked)
}

/// A driver colour's portrait: their kart, parked in the studio, filmed by
/// a camera of its own into a picture.
fn portrait(
    commands: &mut Commands,
    screen: &mut Screen,
    images: &mut Assets<Image>,
    colour: u32,
    slot: usize,
) -> Handle<Image> {
    if let Some((image, _, _)) = screen.portraits.get(&colour) {
        return image.clone();
    }
    let mut target = Image::new_target_texture(PORTRAIT.0 * 2, PORTRAIT.1 * 2, TextureFormat::Rgba8UnormSrgb, None);
    target.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let image = images.add(target);
    let at = STUDIO + Vec3::X * 30.0 * slot as f32;
    commands.spawn((Portrait, crate::karts::Parked { colour: colour as usize }, Transform::from_translation(at)));
    for (lumens, from) in [KEY, FILL] {
        commands.spawn((
            PointLight { intensity: lumens, range: LAMP_RANGE, shadow_maps_enabled: false, ..default() },
            Transform::from_translation(at + from),
        ));
    }
    let camera = commands
        .spawn((
            PortraitCamera,
            Camera3d::default(),
            Camera {
                order: -20 - slot as isize,
                clear_color: ClearColorConfig::Custom(Color::srgb(0.06, 0.08, 0.12)),
                ..default()
            },
            RenderTarget::Image(image.clone().into()),
            bevy::camera::visibility::RenderLayers::layer(STUDIO_LAYER),
            bevy::camera::Exposure { ev100: STUDIO_EV100 },
            Transform::from_translation(at + Vec3::new(1.5, 1.05, 2.3)).looking_at(at + Vec3::Y * 0.5, Vec3::Y),
        ))
        .id();
    screen.portraits.insert(colour, (image.clone(), camera, FILMED));
    image
}

/// Portraits are filmed for a few seconds, then their cameras stop: a
/// picture, not a film.
fn portraits(mut screen: ResMut<Screen>, mut cameras: Query<&mut Camera, With<PortraitCamera>>) {
    for (_, camera, left) in screen.portraits.values_mut() {
        if let Ok(mut settings) = cameras.get_mut(*camera) {
            settings.is_active = *left > 0;
        }
        *left = left.saturating_sub(1);
    }
}

/// Borders: the accent on what is picked, lighter where the pointer is.
fn highlight(
    garage: Res<Garage>,
    screen: Res<Screen>,
    mut cards: Query<(&Card, &Hovered, &mut BorderColor, &mut BackgroundColor)>,
    mut tabs: Query<(&TabOf, &Hovered, &mut BorderColor, &Children), Without<Card>>,
    mut texts: Query<&mut TextColor>,
) {
    for (card, hovered, mut border, mut background) in &mut cards {
        let picked = match card.0 {
            Picked::Event(i) => garage.chosen_event == Some(i),
            Picked::Driver(i) => garage.chosen.0 == i,
            Picked::Kart(i) => garage.chosen.1 == i,
        };
        let colour = if picked {
            theme::ACCENT
        } else if hovered.get() {
            Color::srgba(1.0, 1.0, 1.0, 0.4)
        } else {
            theme::EDGE
        };
        *border = BorderColor::all(colour);
        background.0 = if picked { Color::srgba(1.0, 0.72, 0.2, 0.12) } else { Color::srgba(1.0, 1.0, 1.0, 0.03) };
    }
    for (tab, hovered, mut border, children) in &mut tabs {
        let shown = match tab {
            TabOf::Section(section) => screen.section == *section,
            TabOf::Kind(kind) => screen.tab == Some(*kind),
        };
        let colour = if shown {
            theme::ACCENT
        } else if hovered.get() {
            Color::srgba(1.0, 1.0, 1.0, 0.4)
        } else {
            theme::EDGE
        };
        *border = BorderColor::all(colour);
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(child) {
                text.0 = if shown { theme::TEXT } else { theme::MUTED };
            }
        }
    }
}

/// The spinner turns while the circuit is not there yet or its models load.
fn spin(
    time: Res<Time>,
    course: Option<Res<crate::scene::Course>>,
    waiting: Option<Res<osage_props::Waiting>>,
    mut spinner: Query<&mut Visibility, With<Spinner>>,
    mut ring: Query<&mut UiTransform, With<Ring>>,
) {
    let loading = course.is_none() || waiting.is_some_and(|w| w.0 > 0);
    if let Ok(mut visibility) = spinner.single_mut() {
        visibility.set_if_neq(if loading { Visibility::Inherited } else { Visibility::Hidden });
    }
    if loading && let Ok(mut transform) = ring.single_mut() {
        transform.rotation = Rot2::radians(time.elapsed_secs() * 5.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_has_a_kind_a_title_and_a_map() {
        let tracks = osage_kart_tracks::built_in();
        let events = osage_kart_cup::built_in()
            .iter()
            .map(|cup| format!("cup:{}", cup.id))
            .chain(tracks.iter().map(|t| format!("track:{}", t.id)))
            .chain(tracks.iter().map(|t| format!("trial:{}", t.id)))
            .chain(osage_kart_tracks::arenas().iter().map(|a| format!("battle:{}", a.id)))
            .collect::<Vec<_>>();
        for event in events {
            let title = event_title(&event);
            assert!(!title.is_empty() && !title.contains(':'), "{event}: {title}");
            let map = event_map(&event);
            let lit = map.data.as_ref().unwrap().chunks(4).filter(|p| p[3] > 200).count();
            assert!(lit > 100, "{event}: its circuit drawn ({lit} pixels)");
            let _ = Kind::of(&event);
        }
    }

    #[test]
    fn kinds_by_prefix() {
        assert_eq!(Kind::of("cup:grand-tour"), Kind::Cup);
        assert_eq!(Kind::of("trial:coast"), Kind::Trial);
        assert_eq!(Kind::of("battle:yard"), Kind::Battle);
        assert_eq!(Kind::of("track:forest"), Kind::Race);
    }
}
