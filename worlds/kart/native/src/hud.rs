// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The race on screen: the player's place in large, the lap and the clock,
//! a map of the circuit with every kart on it, the speed and the mini-turbo
//! charging, the item in hand, the start lights' count, the results. It shows a [`RaceView`],
//! whoever fills it.

use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    text::FontSize,
};
use osage_kart_items::Item;
use osage_kart_physics::{TURBO_CHARGES, turbo_level};

use crate::scene::{Course, PALETTE, Stage};

/// Side of the circuit map, pixels.
const MAP: f32 = 190.0;

/// What the interface shows, as the game knows it this frame.
#[derive(Resource, Default)]
pub struct RaceView {
    /// Who and where: world, identity, transport, where the server's source is.
    pub status: String,
    pub stage: Lights,
    pub laps: u32,
    /// Race time, milliseconds.
    pub clock_ms: i64,
    /// Every kart, in race order.
    pub karts: Vec<Entry>,
    /// The player's kart among them, if the player is racing.
    pub me: Option<usize>,
    pub speed: f32,
    pub charge: f32,
    pub boosting: bool,
    /// The engine stalled: a start too early.
    pub stalled: bool,
    /// What the player's kart holds, from a crate.
    pub held: Option<Item>,
    /// How many of it are left (a stack), and whether it is held behind.
    pub count: u8,
    pub trailing: bool,
    /// Seconds of a pit-crew push left.
    pub guard: f32,
    /// The host's name, who starts each race; empty when races start by themselves.
    pub host: String,
    /// The player races with no other driver (bots aside): they may end the race.
    pub alone: bool,
    /// The world's records on this track (D-053): best laps, best races.
    pub records: Records,
    /// Who drives the player's kart and in what, and how to change it (D-055).
    pub pick: String,
}

/// The world's records as the HUD shows them.
#[derive(Default, Clone, PartialEq)]
pub struct Records {
    pub track: String,
    pub race_laps: u32,
    pub laps: Vec<Best>,
    pub races: Vec<Best>,
}

#[derive(Clone, PartialEq)]
pub struct Best {
    pub name: String,
    pub ms: u32,
    /// Set in the latest race.
    pub fresh: bool,
    /// The player's.
    pub mine: bool,
}

#[derive(Default, Clone, Copy, PartialEq)]
pub enum Lights {
    /// Not connected yet, or no race.
    #[default]
    Off,
    Waiting,
    /// On the grid until the host starts the race; `true` if the player is the host.
    Ready(bool),
    /// Seconds to the start.
    Count(i64),
    /// The first second of the race.
    Go,
    Racing,
    /// Seconds to the next race.
    Results(i64),
}

pub struct Entry {
    pub id: u64,
    pub name: String,
    pub bot: bool,
    pub color: u32,
    pub position: u32,
    pub lap: u32,
    pub finish_ms: u64,
    pub at: Vec2,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RaceView>().init_resource::<Dots>().add_systems(Startup, setup).add_systems(
            Update,
            (draw_map.run_if(resource_exists_and_changed::<Course>), write, place_dots).after(Stage::Simulate),
        );
    }
}

#[derive(Component, Clone, Copy, PartialEq)]
enum Label {
    Place,
    Suffix,
    Lap,
    Clock,
    Board,
    Speed,
    Lights,
    Status,
    Held,
    /// What to do now, under the lights: start the race, or wait for the host.
    Prompt,
    /// The world's records, between races.
    Records,
}

/// The panel the records are shown in, hidden while racing.
#[derive(Component)]
struct RecordsPanel;

/// The bar that fills as the mini-turbo charges.
#[derive(Component)]
struct Gauge;

/// The circuit map, and how to place a point of the world on it.
#[derive(Component)]
struct Map;

#[derive(Resource, Default)]
struct Dots {
    /// The corner of the world drawn at the map's top left, and pixels per metre.
    corner: Vec2,
    scale: f32,
    karts: HashMap<u64, Entity>,
}

pub fn clock(ms: i64) -> String {
    let ms = ms.max(0);
    format!("{}:{:05.2}", ms / 60_000, (ms % 60_000) as f32 / 1000.0)
}

fn panel() -> BackgroundColor {
    BackgroundColor(Color::srgba(0.04, 0.06, 0.1, 0.62))
}

fn text(label: Label, size: f32) -> impl Bundle {
    (label, Text::default(), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(Color::WHITE))
}

fn setup(mut commands: Commands) {
    let rounded =
        |node: Node| Node { border_radius: BorderRadius::all(px(12)), padding: UiRect::axes(px(14), px(8)), ..node };
    let corner = |node: Node| rounded(Node { position_type: PositionType::Absolute, ..node });

    // Top left: the lap and the clock.
    commands
        .spawn((
            corner(Node { top: px(14), left: px(14), flex_direction: FlexDirection::Column, ..default() }),
            panel(),
        ))
        .with_children(|lap| {
            lap.spawn(text(Label::Lap, 26.0));
            lap.spawn(text(Label::Clock, 20.0));
        });
    // Bottom left: the place, large.
    let place = Node { bottom: px(46), left: px(18), align_items: AlignItems::FlexEnd, column_gap: px(4), ..default() };
    commands.spawn(Node { position_type: PositionType::Absolute, ..place }).with_children(|place| {
        place.spawn((text(Label::Place, 104.0), TextShadow::default()));
        place.spawn((
            text(Label::Suffix, 40.0),
            TextShadow::default(),
            Node { margin: UiRect::bottom(px(16)), ..default() },
        ));
    });
    // Top right: the order of the race; under it, the map.
    commands
        .spawn((corner(Node { top: px(14), right: px(14), ..default() }), panel()))
        .with_child(text(Label::Board, 15.0));
    commands.spawn((
        Map,
        ImageNode::default(),
        Node {
            position_type: PositionType::Absolute,
            right: px(14),
            bottom: px(96),
            width: px(MAP),
            height: px(MAP),
            ..default()
        },
    ));
    // Bottom right: the speed, and the mini-turbo charging.
    commands
        .spawn((
            corner(Node {
                bottom: px(14),
                right: px(14),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                ..default()
            }),
            panel(),
        ))
        .with_children(|speed| {
            speed.spawn(text(Label::Speed, 24.0));
            let track = Node { width: px(160), height: px(10), border_radius: BorderRadius::all(px(5)), ..default() };
            speed.spawn((track, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.18)))).with_child((
                Gauge,
                Node { width: percent(0), height: percent(100), border_radius: BorderRadius::all(px(5)), ..default() },
                BackgroundColor(Color::WHITE),
            ));
        });
    // Under the lap: the item in hand.
    commands.spawn((
        text(Label::Held, 26.0),
        TextShadow::default(),
        Node { position_type: PositionType::Absolute, top: px(104), left: px(20), ..default() },
    ));
    // The start lights' count and the results, in the middle; who and where, small, at the bottom.
    commands.spawn((
        text(Label::Lights, 120.0),
        TextShadow::default(),
        TextLayout::justify(Justify::Center),
        Node { position_type: PositionType::Absolute, top: percent(16), width: percent(100), ..default() },
    ));
    commands.spawn((
        text(Label::Prompt, 26.0),
        TextShadow::default(),
        TextLayout::justify(Justify::Center),
        Node { position_type: PositionType::Absolute, top: percent(36), width: percent(100), ..default() },
    ));
    // Between races, under the prompt: the world's records.
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, top: percent(46), width: percent(100), ..default() },
            Visibility::Hidden,
            RecordsPanel,
        ))
        .with_children(|row| {
            row.spawn(Node { width: percent(100), justify_content: JustifyContent::Center, ..default() }).with_child((
                rounded(Node::default()),
                panel(),
                children![text(Label::Records, 16.0)],
            ));
        });
    commands.spawn((
        text(Label::Status, 11.0),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(6),
            left: px(14),
            max_width: percent(55),
            ..default()
        },
    ));
}

fn ordinal(place: u32) -> &'static str {
    match (place % 10, place % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    }
}

/// The records as lines: the best laps, then the best races, five each;
/// the player's marked, new ones flagged.
pub fn records_text(records: &Records) -> String {
    let rows = |list: &[Best]| {
        list.iter()
            .take(5)
            .enumerate()
            .map(|(i, best)| {
                let mark = if best.mine { ">" } else { " " };
                let new = if best.fresh { "  NEW!" } else { "" };
                format!("{mark}{:>2} {:<14.14} {:>9}{new}", i + 1, best.name, clock(i64::from(best.ms)))
            })
            .collect::<Vec<_>>()
    };
    let mut out = vec![format!("RECORDS - {}", records.track.to_uppercase()), "Best laps".to_owned()];
    out.extend(rows(&records.laps));
    if !records.races.is_empty() {
        out.push(format!("Best races ({} laps)", records.race_laps));
        out.extend(rows(&records.races));
    }
    out.join("\n")
}

fn write(
    view: Res<RaceView>,
    mut texts: Query<(&Label, &mut Text, &mut TextColor)>,
    mut gauge: Query<(&mut Node, &mut BackgroundColor), With<Gauge>>,
    mut records: Query<&mut Visibility, With<RecordsPanel>>,
) {
    let between = matches!(view.stage, Lights::Waiting | Lights::Ready(_) | Lights::Results(_));
    let shown = between && !view.records.laps.is_empty();
    if let Ok(mut visibility) = records.single_mut() {
        visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
    }
    let me = view.me.and_then(|me| view.karts.get(me));
    for (label, mut text, mut colour) in &mut texts {
        let mut tint = None;
        let wanted = match label {
            Label::Status => view.status.clone(),
            Label::Place => me.map_or(String::new(), |me| me.position.to_string()),
            Label::Suffix => me.map_or(String::new(), |me| ordinal(me.position).to_owned()),
            Label::Lap => match me {
                Some(me) => format!("LAP {}/{}", me.lap.clamp(1, view.laps.max(1)), view.laps),
                None if view.stage == Lights::Off => String::new(),
                None => "WATCHING".to_owned(),
            },
            Label::Clock => match (me, view.stage) {
                (Some(me), _) if me.finish_ms > 0 => format!("finished in {}", clock(me.finish_ms as i64)),
                (None, Lights::Count(_) | Lights::Go | Lights::Racing) => "you race next".to_owned(),
                (_, Lights::Go | Lights::Racing) => clock(view.clock_ms),
                _ => String::new(),
            },
            Label::Board => {
                let rows = view.karts.iter().enumerate().map(|(i, kart)| {
                    let status = if kart.finish_ms > 0 {
                        clock(kart.finish_ms as i64)
                    } else {
                        format!("lap {}", kart.lap.max(1))
                    };
                    let mark = if view.me == Some(i) { ">" } else { " " };
                    let kind = if kart.bot { "bot" } else { "   " };
                    format!("{mark}{:>2} {:<10.10} {kind} {status}", kart.position, kart.name)
                });
                rows.collect::<Vec<_>>().join("\n")
            }
            Label::Speed => match me {
                Some(_) => {
                    format!("{:>3.0} km/h{}", view.speed.abs() * 3.6, if view.boosting { "  TURBO" } else { "" })
                }
                None => String::new(),
            },
            Label::Held => match (me, view.held) {
                (Some(_), Some(item)) => {
                    let (name, colour) = match item {
                        Item::Nitro => ("NITRO", Color::srgb(0.3, 0.8, 1.0)),
                        Item::Tyre => ("TYRE", Color::srgb(0.85, 0.85, 0.85)),
                        Item::Cone => ("CONE", Color::srgb(1.0, 0.5, 0.15)),
                        Item::Oil => ("OIL", Color::srgb(0.75, 0.6, 1.0)),
                        Item::Rocket => ("ROCKET", Color::srgb(1.0, 0.3, 0.25)),
                        Item::Smoke => ("SMOKE", Color::srgb(0.4, 1.0, 0.6)),
                        Item::Stack => ("TYRES", Color::srgb(0.85, 0.85, 0.85)),
                        Item::Drone => ("DRONE", Color::srgb(1.0, 0.35, 0.3)),
                        Item::Bumper => ("BUMPER", Color::srgb(1.0, 0.85, 0.2)),
                        Item::Push => ("PIT PUSH", Color::srgb(1.0, 0.82, 0.3)),
                    };
                    tint = Some(colour);
                    let count = if view.count > 1 { format!(" x{}", view.count) } else { String::new() };
                    // What trails: hold the button to keep it behind as a shield, let go to use it.
                    let how = if !item.trails() {
                        "[F]"
                    } else if view.trailing {
                        "BEHIND - let go of F"
                    } else {
                        "[hold F]"
                    };
                    format!("{name}{count}  {how}")
                }
                (Some(_), None) if view.guard > 0.0 => {
                    tint = Some(Color::srgb(1.0, 0.82, 0.3));
                    "PIT PUSH".to_owned()
                }
                _ => String::new(),
            },
            Label::Lights => match view.stage {
                Lights::Count(seconds) => seconds.max(1).to_string(),
                Lights::Go if view.stalled => {
                    tint = Some(Color::srgb(1.0, 0.3, 0.25));
                    "STALLED".to_owned()
                }
                Lights::Go if view.boosting => {
                    tint = Some(Color::srgb(1.0, 0.75, 0.2));
                    "GREAT START!".to_owned()
                }
                Lights::Go => {
                    tint = Some(Color::srgb(0.35, 1.0, 0.45));
                    "GO!".to_owned()
                }
                Lights::Results(_) if me.is_some_and(|me| me.finish_ms > 0) => "FINISH!".to_owned(),
                Lights::Ready(_) => "READY".to_owned(),
                _ => String::new(),
            },
            Label::Prompt => match view.stage {
                Lights::Ready(true) => {
                    tint = Some(Color::srgb(1.0, 0.85, 0.3));
                    format!("Press Enter to start the race\n(others can still join)\n\n{}", view.pick)
                }
                Lights::Ready(false) => format!("Waiting for {} to start the race\n\n{}", view.host, view.pick),
                Lights::Waiting | Lights::Results(_) => view.pick.clone(),
                Lights::Count(_) if me.is_some() => "Throttle as the lights go out, not before".to_owned(),
                _ => String::new(),
            },
            Label::Records if shown => {
                if view.records.laps.iter().chain(&view.records.races).any(|best| best.fresh && best.mine) {
                    tint = Some(Color::srgb(1.0, 0.85, 0.3));
                }
                records_text(&view.records)
            }
            Label::Records => String::new(),
        };
        if matches!(label, Label::Place | Label::Suffix) {
            // Gold, silver, bronze; then white.
            tint = me.map(|me| match me.position {
                1 => Color::srgb(1.0, 0.84, 0.25),
                2 => Color::srgb(0.85, 0.89, 0.95),
                3 => Color::srgb(0.93, 0.6, 0.35),
                _ => Color::WHITE,
            });
        }
        if text.0 != wanted {
            text.0 = wanted;
        }
        if let Some(tint) = tint.filter(|tint| *tint != colour.0) {
            colour.0 = tint;
        } else if matches!(label, Label::Lights | Label::Records) && tint.is_none() && colour.0 != Color::WHITE {
            colour.0 = Color::WHITE;
        }
    }

    if let Ok((mut bar, mut fill)) = gauge.single_mut() {
        let share = if me.is_some() { (view.charge / TURBO_CHARGES[2]).clamp(0.0, 1.0) } else { 0.0 };
        let width = percent(share * 100.0);
        if bar.width != width {
            bar.width = width;
        }
        // The sparks' colours (effects): blue, orange, violet.
        let stage = match turbo_level(view.charge) {
            3 => Color::srgb(0.75, 0.3, 1.0),
            2 => Color::srgb(1.0, 0.55, 0.1),
            1 => Color::srgb(0.25, 0.65, 1.0),
            _ => Color::srgb(1.0, 0.95, 0.75),
        };
        if fill.0 != stage {
            fill.0 = stage;
        }
    }
}

/// Draws the circuit on the map: a band along its centre line.
fn draw_map(
    course: Res<Course>,
    mut dots: ResMut<Dots>,
    mut images: ResMut<Assets<Image>>,
    mut map: Query<&mut ImageNode, With<Map>>,
) {
    let points: Vec<Vec2> = course.0.track.points().iter().copied().map(Vec2::from).collect();
    let (low, high) = points.iter().fold((Vec2::MAX, Vec2::MIN), |(low, high), p| (low.min(*p), high.max(*p)));
    let margin = 14.0;
    dots.scale = (MAP - 2.0 * margin) / (high - low).max_element().max(1.0);
    // A kart facing +Z has its right at -X: seen from above with +Z up, +X is on the left.
    dots.corner = (low + high) / 2.0 + Vec2::splat(MAP / 2.0 / dots.scale);
    let on_map = |p: Vec2| (dots.corner - p) * dots.scale;

    let size = MAP as usize;
    let mut alpha = vec![0u8; size * size];
    let mut stamp = |at: Vec2, radius: f32, strength: u8| {
        let (r, reach) = (radius.ceil() as i32, radius * radius);
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (at.x as i32 + dx, at.y as i32 + dy);
                if (dx * dx + dy * dy) as f32 <= reach && (0..size as i32).contains(&x) && (0..size as i32).contains(&y)
                {
                    let pixel = &mut alpha[y as usize * size + x as usize];
                    *pixel = (*pixel).max(strength);
                }
            }
        }
    };
    for (i, a) in points.iter().enumerate() {
        let b = points[(i + 1) % points.len()];
        for step in 0..8 {
            stamp(on_map(a.lerp(b, step as f32 / 8.0)), 4.6, 110);
        }
    }
    for (i, a) in points.iter().enumerate() {
        let b = points[(i + 1) % points.len()];
        for step in 0..8 {
            stamp(on_map(a.lerp(b, step as f32 / 8.0)), 2.6, 255);
        }
    }
    let pixels: Vec<u8> =
        alpha.iter().flat_map(|a| if *a == 255 { [255, 255, 255, 235] } else { [10, 14, 24, *a] }).collect();
    let extent = Extent3d { width: size as u32, height: size as u32, depth_or_array_layers: 1 };
    let image =
        Image::new(extent, TextureDimension::D2, pixels, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    if let Ok(mut node) = map.single_mut() {
        node.image = images.add(image);
    }
}

/// A dot per kart on the map; the player's is larger and ringed in white.
fn place_dots(
    mut commands: Commands,
    view: Res<RaceView>,
    mut dots: ResMut<Dots>,
    map: Query<Entity, With<Map>>,
    mut nodes: Query<&mut Node>,
) {
    let Ok(map) = map.single() else { return };
    let (corner, scale) = (dots.corner, dots.scale);
    dots.karts.retain(|id, dot| {
        let racing = view.karts.iter().any(|kart| kart.id == *id);
        if !racing {
            commands.entity(*dot).despawn();
        }
        racing
    });
    for (i, kart) in view.karts.iter().enumerate() {
        let mine = view.me == Some(i);
        let size = if mine { 15.0 } else { 10.0 };
        let at = (corner - kart.at) * scale - Vec2::splat(size / 2.0);
        match dots.karts.get(&kart.id) {
            Some(dot) => {
                if let Ok(mut node) = nodes.get_mut(*dot) {
                    (node.left, node.top) = (px(at.x), px(at.y));
                }
            }
            None => {
                let (r, g, b) = PALETTE[kart.color as usize % PALETTE.len()];
                let node = Node {
                    position_type: PositionType::Absolute,
                    left: px(at.x),
                    top: px(at.y),
                    width: px(size),
                    height: px(size),
                    border: UiRect::all(px(if mine { 2.5 } else { 1.0 })),
                    border_radius: BorderRadius::MAX,
                    ..default()
                };
                let ring = if mine { Color::WHITE } else { Color::srgba(0.0, 0.0, 0.0, 0.6) };
                let dot = commands
                    .spawn((node, BackgroundColor(Color::srgb(r, g, b)), BorderColor::all(ring), ChildOf(map)))
                    .id();
                dots.karts.insert(kart.id, dot);
            }
        }
    }
}
