// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The race on screen: the player's place in large, the lap and the clock,
//! a map of the circuit with every kart on it, the speed and the mini-turbo
//! charging, the item in hand, the start lights' count, the results. It shows a [`RaceView`],
//! whoever fills it.

use std::collections::HashMap;

use bevy::text::FontSource;
use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    text::FontSize,
};
use osage_kart_items::Item;
use osage_kart_physics::{TURBO_CHARGES, turbo_level};

use crate::{
    scene::{Course, PAINTS, Stage},
    theme::{self, Arrows, Faces, Key},
};

/// Side of the circuit map, pixels.
const MAP: f32 = 190.0;

/// What the interface shows, as the game knows it this frame.
#[derive(Resource, Default)]
pub struct RaceView {
    /// Who and where: world, identity, transport, where the server's source is.
    pub status: String,
    pub stage: Lights,
    pub laps: u32,
    /// A time trial (D-062): alone against the clock and the ghosts.
    pub trial: bool,
    /// The player's finish is being shown again, slowed (D-063), or the
    /// whole race (D-082).
    pub replay: bool,
    /// The whole race shown again (D-082): what is followed and the keys;
    /// empty when it is not.
    pub watching: String,
    /// The second player at this computer, on a split screen (D-086).
    pub second: Option<SecondLine>,
    /// A battle (D-065): lives, not laps; the clock counts down.
    pub battle: bool,
    /// The photo mode's help, while it shows (D-066).
    pub photo: String,
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
    /// The cup being raced and its standings, as lines (D-057).
    pub cup: String,
    /// What is raced next and how to choose it, with the votes.
    pub event: String,
}

/// The world's records as the HUD shows them.
#[derive(Default, Clone, PartialEq)]
pub struct Records {
    pub track: String,
    pub race_laps: u32,
    pub laps: Vec<Best>,
    pub races: Vec<Best>,
    /// The best time trials of `race_laps` laps (D-062).
    pub trials: Vec<Best>,
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

/// The second player's place, lap, speed and item, for their line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SecondLine {
    pub position: u32,
    pub lap: u32,
    pub finish_ms: u64,
    pub speed: f32,
    pub held: Option<Item>,
    pub count: u8,
}

pub struct Entry {
    pub id: u64,
    pub name: String,
    pub bot: bool,
    /// A run from an earlier time trial, driven again (D-062).
    pub ghost: bool,
    /// In a battle (D-065): lives left, and whether it is out.
    pub lives: u32,
    pub out: bool,
    pub color: u32,
    pub position: u32,
    pub lap: u32,
    pub finish_ms: u64,
    pub at: Vec2,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<theme::ThemePlugin>() {
            app.add_plugins(theme::ThemePlugin);
        }
        app.init_resource::<RaceView>().init_resource::<Dots>().add_systems(Startup, setup).add_systems(
            Update,
            (draw_map.run_if(resource_exists_and_changed::<Course>), write, board, place_dots, second_panel)
                .after(Stage::Simulate),
        );
    }
}

#[derive(Component, Clone, Copy, PartialEq)]
enum Label {
    Place,
    Suffix,
    Lap,
    Clock,
    Speed,
    Lights,
    Status,
    Held,
    /// What to do now, under the lights: start the race, or wait for the host.
    Prompt,
    /// The world's records, between races.
    Records,
    /// The cup and what is raced next, between races.
    Cup,
    /// Who drives the player's kart and in what.
    Pick,
    /// The second player's line, on a split screen (D-086).
    Second,
}

/// A row of the standings, `0` the leader, and its cells.
#[derive(Component)]
struct Row(usize);

#[derive(Component, Clone, Copy, PartialEq)]
enum Cell {
    Place,
    Chip,
    Name,
    Status,
}

/// The item's panel, shown while the kart holds one.
#[derive(Component)]
struct ItemPanel;

/// The second player's line (D-086).
#[derive(Component)]
struct SecondPanel;

/// Shows the second player's line on a split screen.
fn second_panel(view: Res<RaceView>, mut panels: Query<&mut Visibility, With<SecondPanel>>) {
    let wanted = if view.second.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    for mut visibility in &mut panels {
        visibility.set_if_neq(wanted);
    }
}

/// The start key's hint: only for the host, on the grid.
#[derive(Component)]
struct StartHint;

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

fn text(label: Label, size: f32) -> impl Bundle {
    (label, Text::default(), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(theme::TEXT))
}

/// A text in the thin face.
fn thin(label: Label, size: f32, faces: &Faces) -> impl Bundle {
    (
        label,
        Text::default(),
        TextFont { font: FontSource::Handle(faces.thin.clone()), font_size: FontSize::Px(size), ..default() },
        TextColor(theme::TEXT),
    )
}

/// A fixed caption, small and muted.
fn caption(words: &str, size: f32) -> impl Bundle {
    (Text::new(words), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(theme::MUTED))
}

fn setup(mut commands: Commands, faces: Res<Faces>, arrows: Res<Arrows>) {
    let absolute = |node: Node| Node { position_type: PositionType::Absolute, ..node };

    // Top left: the place, the lap and the clock.
    commands
        .spawn(theme::panel(absolute(Node {
            top: px(14),
            left: px(14),
            column_gap: px(14),
            align_items: AlignItems::Center,
            ..default()
        })))
        .with_children(|p| {
            p.spawn(Node { align_items: AlignItems::FlexEnd, column_gap: px(2), ..default() }).with_children(|place| {
                place.spawn((text(Label::Place, 52.0), TextShadow::default()));
                place.spawn((text(Label::Suffix, 20.0), Node { margin: UiRect::bottom(px(8)), ..default() }));
            });
            p.spawn((Node { width: px(2), height: px(44), ..default() }, BackgroundColor(theme::ACCENT)));
            p.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }).with_children(|c| {
                c.spawn(text(Label::Lap, 20.0));
                c.spawn(thin(Label::Clock, 18.0, &faces));
            });
        });
    // Top middle: the item in hand.
    commands
        .spawn(absolute(Node {
            top: px(14),
            width: percent(100),
            justify_content: JustifyContent::Center,
            ..default()
        }))
        .with_children(|row| {
            row.spawn((
                theme::panel(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    min_width: px(170),
                    row_gap: px(2),
                    ..default()
                }),
                ItemPanel,
                Visibility::Hidden,
            ))
            .with_children(|item| {
                item.spawn(caption("ITEM", 11.0));
                item.spawn(text(Label::Held, 20.0));
            });
        });
    // Between the two halves of a split screen, a dark rule (D-086).
    commands.spawn((
        SecondPanel,
        absolute(Node {
            top: percent(50),
            left: px(0),
            right: px(0),
            height: px(4),
            margin: UiRect::top(px(-2)),
            ..default()
        }),
        BackgroundColor(Color::srgb(0.02, 0.025, 0.035)),
        Visibility::Hidden,
    ));
    // The second player's line, at the top of their half (D-086).
    commands
        .spawn((
            SecondPanel,
            theme::panel(absolute(Node { top: percent(50), left: px(14), margin: UiRect::top(px(10)), ..default() })),
            Visibility::Hidden,
        ))
        .with_children(|panel| {
            panel.spawn(text(Label::Second, 16.0));
        });
    // Top right: the standings, a row each kart.
    commands
        .spawn(theme::panel(absolute(Node {
            top: px(14),
            right: px(14),
            flex_direction: FlexDirection::Column,
            row_gap: px(3),
            min_width: px(220),
            ..default()
        })))
        .with_children(|board| {
            board.spawn((caption("STANDINGS", 11.0), Node { margin: UiRect::bottom(px(4)), ..default() }));
            for i in 0..8 {
                board
                    .spawn((
                        Row(i),
                        Node {
                            display: Display::None,
                            align_items: AlignItems::Center,
                            column_gap: px(8),
                            padding: UiRect::axes(px(6), px(2)),
                            border_radius: BorderRadius::all(px(5)),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ))
                    .with_children(|row| {
                        let cell = |cell: Cell, width: f32, size: f32, faces: &Faces| {
                            (
                                cell,
                                Text::default(),
                                TextFont {
                                    font: if cell == Cell::Name {
                                        FontSource::Handle(faces.thin.clone())
                                    } else {
                                        default()
                                    },
                                    font_size: FontSize::Px(size),
                                    ..default()
                                },
                                TextColor(theme::TEXT),
                                Node { width: px(width), ..default() },
                            )
                        };
                        row.spawn(cell(Cell::Place, 18.0, 14.0, &faces));
                        row.spawn((
                            Cell::Chip,
                            Node { width: px(8), height: px(14), border_radius: BorderRadius::all(px(2)), ..default() },
                            BackgroundColor(Color::WHITE),
                        ));
                        row.spawn(cell(Cell::Name, 110.0, 13.0, &faces));
                        row.spawn((cell(Cell::Status, 64.0, 13.0, &faces), TextLayout::justify(Justify::Right)));
                    });
            }
        });
    commands.spawn((
        Map,
        ImageNode::default(),
        absolute(Node { right: px(14), bottom: px(96), width: px(MAP), height: px(MAP), ..default() }),
    ));
    // Bottom right: the speed, and the mini-turbo charging.
    commands
        .spawn(theme::panel(absolute(Node {
            bottom: px(14),
            right: px(14),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            ..default()
        })))
        .with_children(|speed| {
            speed.spawn(Node { align_items: AlignItems::FlexEnd, column_gap: px(6), ..default() }).with_children(
                |figure| {
                    figure.spawn(text(Label::Speed, 34.0));
                    figure.spawn((caption("KM/H", 12.0), Node { margin: UiRect::bottom(px(5)), ..default() }));
                },
            );
            let track = Node { width: px(150), height: px(8), border_radius: BorderRadius::all(px(4)), ..default() };
            speed.spawn((track, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.14)))).with_child((
                Gauge,
                Node { width: percent(0), height: percent(100), border_radius: BorderRadius::all(px(4)), ..default() },
                BackgroundColor(Color::WHITE),
            ));
        });
    // The start lights' count and the results, in the middle; what to do, under them.
    commands.spawn((
        text(Label::Lights, 110.0),
        TextShadow { offset: Vec2::splat(4.0), ..default() },
        TextLayout::justify(Justify::Center),
        absolute(Node { top: percent(14), width: percent(100), ..default() }),
    ));
    commands.spawn((
        thin(Label::Prompt, 22.0, &faces),
        TextShadow::default(),
        TextLayout::justify(Justify::Center),
        absolute(Node { top: percent(34), width: percent(100), ..default() }),
    ));
    // Between races: the event, the records and the player's kart, as
    // cards side by side; the keys under them.
    commands
        .spawn((
            absolute(Node {
                top: percent(42),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(12),
                ..default()
            }),
            Visibility::Hidden,
            RecordsPanel,
        ))
        .with_children(|between| {
            between.spawn(Node { column_gap: px(12), align_items: AlignItems::FlexStart, ..default() }).with_children(
                |cards| {
                    for (title, label) in
                        [("EVENT", Label::Cup), ("RECORDS", Label::Records), ("YOUR KART", Label::Pick)]
                    {
                        cards
                            .spawn(theme::panel(Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: px(8),
                                width: px(300),
                                ..default()
                            }))
                            .with_children(|card| {
                                card.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() })
                                    .with_children(|head| {
                                        head.spawn((
                                            Node { width: px(3), height: px(14), ..default() },
                                            BackgroundColor(theme::ACCENT),
                                        ));
                                        head.spawn((
                                            Text::new(title),
                                            TextFont { font_size: FontSize::Px(14.0), ..default() },
                                            TextColor(theme::ACCENT),
                                        ));
                                    });
                                card.spawn(thin(label, 13.0, &faces));
                            });
                    }
                },
            );
            // The keys, as keycaps.
            between
                .spawn(theme::panel(Node { column_gap: px(18), align_items: AlignItems::Center, ..default() }))
                .with_children(|bar| {
                    let start = theme::hint(bar, &[Key::Word("ENTER")], "START", &arrows);
                    bar.commands().entity(start).insert(StartHint);
                    theme::hint(bar, &[Key::Arrow(3), Key::Arrow(1)], "DRIVER", &arrows);
                    theme::hint(bar, &[Key::Arrow(0), Key::Arrow(2)], "KART", &arrows);
                    theme::hint(bar, &[Key::Word("P")], "PAINT", &arrows);
                    theme::hint(bar, &[Key::Word("T")], "EVENT", &arrows);
                    theme::hint(bar, &[Key::Word("R")], "REPLAY", &arrows);
                    theme::hint(bar, &[Key::Word("ESC")], "SETTINGS", &arrows);
                });
        });
    commands
        .spawn((
            thin(Label::Status, 10.0, &faces),
            absolute(Node { bottom: px(6), left: px(14), max_width: percent(55), ..default() }),
        ))
        .insert(TextColor(theme::MUTED));
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

/// The records as lines: the best laps, then the best races and the best
/// time trials, five each; the player's marked, new ones flagged.
pub fn records_text(records: &Records) -> String {
    let rows = |list: &[Best]| {
        list.iter()
            .take(5)
            .enumerate()
            .map(|(i, best)| {
                let mark = if best.mine { "> " } else { "" };
                let new = if best.fresh { "   NEW!" } else { "" };
                format!("{}.  {mark}{}   {}{new}", i + 1, best.name, clock(i64::from(best.ms)))
            })
            .collect::<Vec<_>>()
    };
    let mut out = vec![records.track.to_uppercase(), String::new(), "BEST LAPS".to_owned()];
    out.extend(rows(&records.laps));
    if !records.races.is_empty() {
        out.extend([String::new(), format!("BEST RACES, {} LAPS", records.race_laps)]);
        out.extend(rows(&records.races));
    }
    if !records.trials.is_empty() {
        out.extend([String::new(), format!("BEST TIME TRIALS, {} LAPS", records.race_laps)]);
        out.extend(rows(&records.trials));
    }
    out.join("\n")
}

#[allow(clippy::type_complexity)]
fn write(
    view: Res<RaceView>,
    mut texts: Query<(&Label, &mut Text, &mut TextColor)>,
    mut gauge: Query<(&mut Node, &mut BackgroundColor), With<Gauge>>,
    mut records: Query<&mut Visibility, (With<RecordsPanel>, Without<ItemPanel>, Without<StartHint>)>,
    mut item: Query<&mut Visibility, (With<ItemPanel>, Without<StartHint>)>,
    mut start: Query<&mut Visibility, With<StartHint>>,
) {
    let between = matches!(view.stage, Lights::Waiting | Lights::Ready(_) | Lights::Results(_));
    // Hidden while the whole race is shown again: the picture is the point.
    let shown = between
        && view.watching.is_empty()
        && !(view.records.laps.is_empty() && view.cup.is_empty() && view.pick.is_empty());
    let show = |on: bool| if on { Visibility::Inherited } else { Visibility::Hidden };
    if let Ok(mut visibility) = records.single_mut() {
        visibility.set_if_neq(show(shown));
    }
    if let Ok(mut visibility) = item.single_mut() {
        visibility.set_if_neq(show(view.held.is_some() || view.guard > 0.0));
    }
    if let Ok(mut visibility) = start.single_mut() {
        visibility.set_if_neq(show(view.stage == Lights::Ready(true)));
    }
    let me = view.me.and_then(|me| view.karts.get(me));
    for (label, mut text, mut colour) in &mut texts {
        let mut tint = None;
        let wanted = match label {
            Label::Status => view.status.clone(),
            Label::Place => me.map_or(String::new(), |me| me.position.to_string()),
            Label::Suffix => me.map_or(String::new(), |me| ordinal(me.position).to_owned()),
            Label::Lap => match me {
                Some(me) if view.battle && me.out => "OUT".to_owned(),
                Some(me) if view.battle => lives(me.lives).to_uppercase(),
                Some(me) if view.trial => format!("TRIAL {}/{}", me.lap.clamp(1, view.laps.max(1)), view.laps),
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
            Label::Speed => match me {
                Some(_) => {
                    format!("{:.0}", view.speed.abs() * 3.6)
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
                _ if view.replay => {
                    tint = Some(Color::srgb(1.0, 0.85, 0.3));
                    "REPLAY".to_owned()
                }
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
                Lights::Results(_) if view.battle && me.is_some_and(|me| me.position == 1) => {
                    tint = Some(Color::srgb(1.0, 0.84, 0.25));
                    "WINNER!".to_owned()
                }
                Lights::Results(_) if me.is_some_and(|me| me.finish_ms > 0) => "FINISH!".to_owned(),
                Lights::Ready(_) => "READY".to_owned(),
                _ => String::new(),
            },
            Label::Prompt if !view.photo.is_empty() => {
                tint = Some(Color::srgb(1.0, 0.85, 0.3));
                view.photo.clone()
            }
            Label::Second => match &view.second {
                Some(second) => {
                    let place = if second.position > 0 {
                        format!("{}{}", second.position, ordinal(second.position).to_uppercase())
                    } else {
                        "-".to_owned()
                    };
                    let progress = if second.finish_ms > 0 {
                        format!("FINISHED {}", clock(second.finish_ms as i64))
                    } else {
                        format!("LAP {}/{}", second.lap.clamp(1, view.laps.max(1)), view.laps.max(1))
                    };
                    let item = second.held.map_or(String::new(), |held| {
                        let count = if second.count > 1 { format!(" x{}", second.count) } else { String::new() };
                        format!("   {}{count} [RIGHT CTRL]", format!("{held:?}").to_uppercase())
                    });
                    format!("P2   {place}   {progress}   {:.0} KM/H{item}", second.speed.abs() * 3.6)
                }
                None => String::new(),
            },
            Label::Prompt => match view.stage {
                _ if !view.watching.is_empty() => view.watching.clone(),
                Lights::Ready(true) => {
                    tint = Some(Color::srgb(1.0, 0.85, 0.3));
                    "Press Enter to start the race\n(others can still join)".to_owned()
                }
                Lights::Ready(false) => format!("Waiting for {} to start the race", view.host),
                Lights::Count(_) if me.is_some() => "Throttle as the lights go out, not before".to_owned(),
                _ => String::new(),
            },
            Label::Records if shown && !view.records.laps.is_empty() => {
                let records = &view.records;
                if records.laps.iter().chain(&records.races).chain(&records.trials).any(|b| b.fresh && b.mine) {
                    tint = Some(theme::ACCENT);
                }
                records_text(records)
            }
            Label::Records => "No lap set here yet".to_owned(),
            Label::Cup if shown => [view.cup.as_str(), view.event.as_str()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n"),
            Label::Cup => String::new(),
            Label::Pick if shown => view.pick.clone(),
            Label::Pick => String::new(),
        };
        if matches!(label, Label::Place | Label::Suffix) {
            // Gold, silver, bronze; then white.
            tint = me.map(|me| theme::place_colour(me.position));
        }
        if text.0 != wanted {
            text.0 = wanted;
        }
        if let Some(tint) = tint.filter(|tint| *tint != colour.0) {
            colour.0 = tint;
        } else if matches!(label, Label::Lights | Label::Records) && tint.is_none() && colour.0 != theme::TEXT {
            colour.0 = theme::TEXT;
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

/// A battle kart's lives, in words: a row of marks reads poorly small.
fn lives(left: u32) -> String {
    match left {
        1 => "1 life".to_owned(),
        n => format!("{n} lives"),
    }
}

/// The standings, a row each kart: place, colour, name, how it stands.
#[allow(clippy::type_complexity)]
fn board(
    view: Res<RaceView>,
    mut rows: Query<(&Row, &mut Node, &mut BackgroundColor, &Children), Without<Cell>>,
    mut cells: Query<(&Cell, Option<&mut Text>, Option<&mut TextColor>, Option<&mut BackgroundColor>), With<Cell>>,
) {
    for (row, mut node, mut back, children) in &mut rows {
        // Rows past the field take no room: the panel is as tall as it.
        let wanted = if view.karts.get(row.0).is_some() { Display::Flex } else { Display::None };
        if node.display != wanted {
            node.display = wanted;
        }
        let Some(kart) = view.karts.get(row.0) else { continue };
        let mine = view.me == Some(row.0);
        let wanted = if mine { Color::srgba(1.0, 0.72, 0.2, 0.22) } else { Color::NONE };
        if back.0 != wanted {
            back.0 = wanted;
        }
        // A ghost shows the time it drives to: its driver's best trial.
        let best = view.records.trials.iter().find(|best| kart.ghost && best.name == kart.name);
        let status = if view.battle {
            if kart.out { "OUT".to_owned() } else { lives(kart.lives).to_uppercase() }
        } else if kart.finish_ms > 0 {
            clock(kart.finish_ms as i64)
        } else if let Some(best) = best {
            clock(i64::from(best.ms))
        } else {
            format!("LAP {}", kart.lap.clamp(1, view.laps.max(1)))
        };
        let name = match (kart.ghost, kart.bot) {
            (true, _) => format!("{} (ghost)", kart.name),
            _ => kart.name.clone(),
        };
        for child in children.iter() {
            let Ok((cell, text, colour, fill)) = cells.get_mut(child) else { continue };
            let (words, tint) = match cell {
                Cell::Place => (kart.position.to_string(), theme::place_colour(kart.position)),
                Cell::Name => (name.clone(), if kart.out { theme::MUTED } else { theme::TEXT }),
                Cell::Status => (status.clone(), theme::MUTED),
                Cell::Chip => {
                    let (r, g, b) = PAINTS[kart.color as usize % PAINTS.len()];
                    if let Some(mut fill) = fill {
                        let paint = Color::srgb(r, g, b);
                        if fill.0 != paint {
                            fill.0 = paint;
                        }
                    }
                    continue;
                }
            };
            if let Some(mut text) = text
                && text.0 != words
            {
                text.0 = words;
            }
            if let Some(mut colour) = colour
                && colour.0 != tint
            {
                colour.0 = tint;
            }
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
                let (r, g, b) = PAINTS[kart.color as usize % PAINTS.len()];
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
