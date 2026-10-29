//! Streams fake snapshots on the same port as the real SKSE plugin so the
//! companion UI can be developed without launching Skyrim.
//!
//! Usage: `cargo run -p mock-skse [port]`

use protocol::*;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

fn main() -> std::io::Result<()> {
    let port = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!("mock-skse listening on 127.0.0.1:{port}");
    let start = Instant::now();
    for stream in listener.incoming() {
        let stream = stream?;
        println!("client connected: {:?}", stream.peer_addr());
        std::thread::spawn(move || {
            if let Err(e) = serve(stream, start) {
                println!("client gone: {e}");
            }
        });
    }
    Ok(())
}

fn serve(mut stream: TcpStream, start: Instant) -> std::io::Result<()> {
    stream.set_nodelay(true)?;
    loop {
        let t = start.elapsed().as_secs_f32();
        let line = encode_line(&fake_snapshot(t)).expect("serialize");
        stream.write_all(line.as_bytes())?;
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn fake_snapshot(t: f32) -> Snapshot {
    // Walk a slow circle around Whiterun; one in-game hour every 10 real seconds.
    let (cx, cy, r) = (20_000.0, -10_000.0, 8_000.0);
    let a = t * 0.05;
    let hour = (8.0 + t / 10.0) % 24.0;
    let xp_next = 375.0;
    Snapshot {
        protocol: PROTOCOL_VERSION,
        game_version: "mock".into(),
        plugin_version: "mock".into(),
        in_game: true,
        player: Player {
            name: "Dovahkiin".into(),
            level: 14,
            xp: (t * 3.0) % xp_next,
            xp_next,
            position: Vec3 {
                x: cx + r * a.sin(),
                y: cy + r * a.cos(),
                z: 0.0,
            },
            heading: (a + std::f32::consts::FRAC_PI_2).rem_euclid(std::f32::consts::TAU),
            // Look around a little relative to the walking direction, like a third-person camera.
            camera_heading: Some(
                (a + std::f32::consts::FRAC_PI_2 + 0.8 * (t * 0.5).sin())
                    .rem_euclid(std::f32::consts::TAU),
            ),
            gold: 1_234,
            // Swings through all three load colours over time.
            carry_weight: 300.0 * (0.6 + 0.5 * (t * 0.05).sin().abs()),
            max_carry_weight: 300.0,
            worldspace: "Tamriel".into(),
            cell: "Wilderness".into(),
            location: "Whiterun Hold".into(),
            is_interior: false,
        },
        time: GameTime {
            hour,
            day: 17,
            month: 7,
            year: 201,
            day_of_week: 3,
            days_passed: 3.0 + t / 240.0,
            timescale: 20.0,
        },
        crime: Crime {
            bounties: if (t as u32 / 30) % 2 == 1 {
                vec![Bounty {
                    faction: "Whiterun".into(),
                    violent: 0,
                    non_violent: 40,
                }]
            } else {
                vec![]
            },
        },
        quests: vec![
            Quest {
                form_id: 0x0002_610C,
                editor_id: "MQ103".into(),
                name: "Dragon Rising".into(),
                kind: QuestKind::Main,
                tracked: true,
                status: QuestStatus::Tracked,
                objectives: vec![
                    Objective {
                        index: 10,
                        text: "Meet Irileth near the Western Watchtower".into(),
                        state: ObjectiveState::Completed,
                    },
                    Objective {
                        index: 20,
                        text: "Kill the dragon".into(),
                        state: ObjectiveState::Active,
                    },
                ],
            },
            Quest {
                form_id: 0x0001_F251,
                editor_id: "MS13".into(),
                name: "In My Time of Need".into(),
                kind: QuestKind::Side,
                tracked: false,
                status: QuestStatus::Current,
                objectives: vec![Objective {
                    index: 10,
                    text: "Find the Redguard woman".into(),
                    state: ObjectiveState::Active,
                }],
            },
            Quest {
                form_id: 0x0003_A2C5,
                editor_id: "C00".into(),
                name: "Take Up Arms".into(),
                kind: QuestKind::Faction,
                tracked: false,
                status: QuestStatus::Current,
                objectives: vec![Objective {
                    index: 10,
                    text: "Talk to Kodlak Whitemane".into(),
                    state: ObjectiveState::Active,
                }],
            },
            Quest {
                form_id: 0x0006_5BDF,
                editor_id: "FreeformRiverwoodA".into(),
                name: "Lost Lumber".into(),
                kind: QuestKind::Misc,
                tracked: false,
                status: QuestStatus::Current,
                // An unresolved counter the way an older plugin sent it.
                objectives: vec![Objective {
                    index: 10,
                    text: "Gather firewood (<Global=M_GlobalFirewood>/<Global=FirewoodTotal>)"
                        .into(),
                    state: ObjectiveState::Active,
                }],
            },
            Quest {
                form_id: 0x000E_7B0A,
                editor_id: "MQ101".into(),
                name: "Unbound".into(),
                kind: QuestKind::Main,
                tracked: false,
                status: QuestStatus::Completed,
                objectives: vec![
                    Objective {
                        index: 10,
                        text: "Escape Helgen".into(),
                        state: ObjectiveState::Completed,
                    },
                    Objective {
                        index: 20,
                        text: "Follow Ralof".into(),
                        state: ObjectiveState::Completed,
                    },
                ],
            },
            Quest {
                form_id: 0x0002_5F2B,
                editor_id: "DA02".into(),
                name: "Boethiah's Calling".into(),
                kind: QuestKind::Daedric,
                tracked: false,
                status: QuestStatus::Unknown,
                objectives: vec![],
            },
            Quest {
                form_id: 0x0002_1D41,
                editor_id: "TG01".into(),
                name: "A Chance Arrangement".into(),
                kind: QuestKind::Faction,
                tracked: false,
                status: QuestStatus::Unknown,
                objectives: vec![],
            },
        ],
        markers: vec![
            Marker {
                name: "Western Watchtower".into(),
                kind: MarkerKind::QuestTarget,
                position: Vec3 {
                    x: 10_000.0,
                    y: -12_000.0,
                    z: 0.0,
                },
                quest: Some(0x0002_610C),
            },
            Marker {
                name: "Whiterun".into(),
                kind: MarkerKind::Location,
                position: Vec3 {
                    x: 20_000.0,
                    y: -10_000.0,
                    z: 0.0,
                },
                quest: None,
            },
        ],
        places: mock_places(cx, cy),
        session: 1,
        events: mock_events(t, cx, cy, r),
    }
}

/// Every 15 s something happens a little behind the player: a kill, a theft, a standing stone, ...
/// Each one comes as a pair a couple of metres apart, so the app should merge it into one spot.
fn mock_events(t: f32, cx: f32, cy: f32, r: f32) -> Vec<GameEvent> {
    const KINDS: [(&str, &str); 4] = [
        ("kill", "Killed Wolf"),
        ("steal", "Stole Silver Goblet"),
        ("standing_stone", "Activated The Warrior Stone"),
        ("discovered", "Discovered Halted Stream Camp"),
    ];
    let happened = (t / 15.0) as u64;
    (happened.saturating_sub(10)..happened)
        .flat_map(|k| {
            let a = (k as f32 * 15.0 + 10.0) * 0.05;
            let (kind, text) = KINDS[k as usize % KINDS.len()];
            (0..2).map(move |i| GameEvent {
                seq: k * 2 + i + 1,
                kind: kind.into(),
                text: text.into(),
                player: "Dovahkiin".into(),
                position: Vec3 {
                    x: cx + r * a.sin() + i as f32 * 150.0,
                    y: cy + r * a.cos(),
                    z: 0.0,
                },
                worldspace: "Tamriel".into(),
                is_interior: false,
                days_passed: 3.0 + k as f32 / 16.0,
            })
        })
        .collect()
}

/// One place of every icon family, in a ring around the route, so the map icons can be checked.
fn mock_places(cx: f32, cy: f32) -> Vec<Place> {
    let kinds = [
        ("Whiterun", "city"),
        ("Riverwood", "town"),
        ("Pelagia Farm", "farm"),
        ("Bleak Falls Barrow", "nordic_ruin"),
        ("Embershard Mine", "mine"),
        ("Halted Stream Camp", "camp"),
        ("Fort Greymoor", "fort"),
        ("Graywinter Watch", "cave"),
        ("Mzinchaleft", "dwemer_ruin"),
        ("Western Watchtower", "dragon_lair"),
        ("The Warrior Stone", "standing_stone"),
        ("Solitude Lighthouse", "lighthouse"),
        ("Northwind Wreck", "shipwreck"),
        ("Eldergleam Sanctuary", "grove"),
        ("Pass", "pass"),
        ("Hall of the Vigilant", "landmark"),
    ];
    let n = kinds.len() as f32;
    let mut places: Vec<Place> = kinds
        .iter()
        .enumerate()
        .map(|(i, (name, kind))| {
            let a = i as f32 / n * std::f32::consts::TAU;
            Place {
                name: (*name).into(),
                kind: (*kind).into(),
                position: Vec3 {
                    x: cx + 14_000.0 * a.sin(),
                    y: cy + 14_000.0 * a.cos(),
                    z: 0.0,
                },
                discovered: i % 5 != 4,
            }
        })
        .collect();
    // On the walking loop, so its name shows whenever the player passes within 25 m.
    places.push(Place {
        name: "Honningbrew Meadery".into(),
        kind: "settlement".into(),
        position: Vec3 {
            x: cx,
            y: cy + 8_000.0,
            z: 0.0,
        },
        discovered: true,
    });
    places
}
