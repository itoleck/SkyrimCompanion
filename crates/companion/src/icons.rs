//! Map location icons in the style of Skyrim's world map: bone-white silhouettes on a dark
//! disc, drawn as vector shapes so they stay crisp at any zoom.
//!
//! Shapes are designed on a -1..1 grid (y down) and scaled to the icon radius.

use egui::{Color32, Painter, Pos2, Shape, Stroke, Vec2};
use serde::{Deserialize, Serialize};

pub const BONE: Color32 = Color32::from_rgb(232, 220, 192);
const SHADOW: Color32 = Color32::from_rgba_premultiplied(14, 10, 6, 200);
const HOLE: Color32 = Color32::from_rgb(28, 22, 16);

/// Icon family for a place kind as sent by the plugin (`Place::kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Icon {
    City,
    Town,
    Hut,
    Cave,
    NordicRuin,
    DwemerRuin,
    Tower,
    Camp,
    Mine,
    StandingStone,
    DragonLair,
    Lighthouse,
    Ship,
    Tree,
    Pass,
    Landmark,
}

impl Icon {
    pub const ALL: [Icon; 16] = [
        Icon::City,
        Icon::Town,
        Icon::Hut,
        Icon::Cave,
        Icon::NordicRuin,
        Icon::DwemerRuin,
        Icon::Tower,
        Icon::Camp,
        Icon::Mine,
        Icon::StandingStone,
        Icon::DragonLair,
        Icon::Lighthouse,
        Icon::Ship,
        Icon::Tree,
        Icon::Pass,
        Icon::Landmark,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Icon::City => "City",
            Icon::Town => "Town",
            Icon::Hut => "House / farm",
            Icon::Cave => "Cave",
            Icon::NordicRuin => "Nordic ruin",
            Icon::DwemerRuin => "Dwemer ruin",
            Icon::Tower => "Fort / tower",
            Icon::Camp => "Camp",
            Icon::Mine => "Mine",
            Icon::StandingStone => "Shrine / stone",
            Icon::DragonLair => "Dragon lair",
            Icon::Lighthouse => "Lighthouse",
            Icon::Ship => "Ship / docks",
            Icon::Tree => "Grove",
            Icon::Pass => "Pass",
            Icon::Landmark => "Landmark",
        }
    }

    pub fn for_kind(kind: &str) -> Self {
        match kind {
            "city" => Icon::City,
            "town" => Icon::Town,
            "settlement" | "farm" | "shack" | "nordic_dwelling" | "stable" | "wheat_mill"
            | "wood_mill" => Icon::Hut,
            "cave" => Icon::Cave,
            "nordic_ruin" => Icon::NordicRuin,
            "dwemer_ruin" => Icon::DwemerRuin,
            "fort" | "imperial_tower" | "nordic_tower" | "orc_stronghold" => Icon::Tower,
            "camp" | "imperial_camp" | "stormcloak_camp" | "giant_camp" => Icon::Camp,
            "mine" | "smelter" => Icon::Mine,
            "shrine" | "altar" | "standing_stone" => Icon::StandingStone,
            "dragon_lair" => Icon::DragonLair,
            "lighthouse" => Icon::Lighthouse,
            "shipwreck" | "docks" => Icon::Ship,
            "grove" | "clearing" => Icon::Tree,
            "pass" => Icon::Pass,
            _ => Icon::Landmark,
        }
    }
}

/// Draws `icon` centred at `at` with radius `r`. `faded` is for places seen but not yet visited.
pub fn draw(painter: &Painter, icon: Icon, at: Pos2, r: f32, faded: bool) {
    let fill = if faded {
        BONE.gamma_multiply(0.5)
    } else {
        BONE
    };
    painter.circle(
        at,
        r * 1.3,
        SHADOW,
        Stroke::new(1.0, fill.gamma_multiply(0.45)),
    );

    let s = r * 0.8;
    let p = |x: f32, y: f32| at + Vec2::new(x, y) * s;
    let poly = |pts: &[(f32, f32)], color: Color32| {
        painter.add(Shape::convex_polygon(
            pts.iter().map(|&(x, y)| p(x, y)).collect(),
            color,
            Stroke::NONE,
        ));
    };
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32, color: Color32| {
        poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], color)
    };
    let line = |pts: &[(f32, f32)], width: f32, color: Color32| {
        painter.add(Shape::line(
            pts.iter().map(|&(x, y)| p(x, y)).collect(),
            Stroke::new(width * s / 8.0, color),
        ));
    };

    match icon {
        Icon::City => {
            // Keep with three crenellated towers and a gate.
            rect(-0.8, -0.15, 0.8, 0.9, fill);
            rect(-0.95, -0.6, -0.5, 0.9, fill);
            rect(0.5, -0.6, 0.95, 0.9, fill);
            rect(-0.25, -0.95, 0.25, 0.9, fill);
            for x in [-0.95, -0.65, 0.5, 0.8] {
                rect(x, -0.78, x + 0.15, -0.6, fill);
            }
            rect(-0.25, -1.1, -0.1, -0.95, fill);
            rect(0.1, -1.1, 0.25, -0.95, fill);
            poly(
                &[
                    (-0.2, 0.9),
                    (-0.2, 0.45),
                    (0.0, 0.25),
                    (0.2, 0.45),
                    (0.2, 0.9),
                ],
                HOLE,
            );
        }
        Icon::Town => longhouse(&poly, &rect, &line, 1.0, fill),
        Icon::Hut => longhouse(&poly, &rect, &line, 0.75, fill),
        Icon::Cave => {
            // Rocky mound with a dark mouth.
            poly(
                &[
                    (-1.0, 0.8),
                    (-0.85, 0.1),
                    (-0.45, -0.45),
                    (0.0, -0.65),
                    (0.45, -0.45),
                    (0.85, 0.1),
                    (1.0, 0.8),
                ],
                fill,
            );
            poly(
                &[
                    (-0.45, 0.8),
                    (-0.4, 0.3),
                    (0.0, 0.0),
                    (0.4, 0.3),
                    (0.45, 0.8),
                ],
                HOLE,
            );
        }
        Icon::NordicRuin => {
            // Barrow door: two pillars, a lintel and the claw-like spike above it.
            rect(-0.85, -0.3, -0.45, 0.9, fill);
            rect(0.45, -0.3, 0.85, 0.9, fill);
            rect(-1.0, -0.55, 1.0, -0.3, fill);
            poly(&[(-0.35, -0.55), (0.0, -1.05), (0.35, -0.55)], fill);
            poly(
                &[(-0.45, -0.3), (0.45, -0.3), (0.45, 0.9), (-0.45, 0.9)],
                HOLE,
            );
        }
        Icon::DwemerRuin => {
            // Cog wheel.
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let t = |rr: f32, da: f32| {
                    let (sin, cos) = (a + da).sin_cos();
                    (cos * rr, sin * rr)
                };
                poly(
                    &[t(0.55, -0.2), t(0.98, -0.14), t(0.98, 0.14), t(0.55, 0.2)],
                    fill,
                );
            }
            painter.circle_filled(at, 0.7 * s, fill);
            painter.circle_filled(at, 0.28 * s, HOLE);
        }
        Icon::Tower => {
            rect(-0.5, -0.55, 0.5, 0.95, fill);
            rect(-0.7, -0.75, 0.7, -0.5, fill);
            for x in [-0.7, -0.1, 0.5] {
                rect(x, -1.0, x + 0.2, -0.75, fill);
            }
            rect(-0.08, -0.3, 0.08, 0.15, HOLE);
            poly(
                &[
                    (-0.2, 0.95),
                    (-0.2, 0.6),
                    (0.0, 0.45),
                    (0.2, 0.6),
                    (0.2, 0.95),
                ],
                HOLE,
            );
        }
        Icon::Camp => {
            // Hide tent with crossed poles.
            line(&[(-0.35, -1.05), (0.25, -0.35)], 1.6, fill);
            line(&[(0.35, -1.05), (-0.25, -0.35)], 1.6, fill);
            poly(&[(-0.95, 0.85), (0.0, -0.7), (0.95, 0.85)], fill);
            poly(&[(-0.3, 0.85), (0.0, 0.2), (0.3, 0.85)], HOLE);
        }
        Icon::Mine => {
            // Pickaxe.
            line(&[(-0.75, 0.85), (0.35, -0.3)], 2.2, fill);
            line(
                &[
                    (-0.45, -0.85),
                    (0.0, -0.75),
                    (0.4, -0.5),
                    (0.7, -0.1),
                    (0.85, 0.4),
                ],
                2.2,
                fill,
            );
        }
        Icon::StandingStone => {
            poly(
                &[
                    (-0.45, 0.95),
                    (-0.55, -0.4),
                    (-0.25, -0.95),
                    (0.25, -0.95),
                    (0.55, -0.4),
                    (0.45, 0.95),
                ],
                fill,
            );
            // Carved rune.
            line(&[(0.0, -0.6), (0.0, 0.6)], 1.2, HOLE);
            line(&[(0.0, -0.3), (-0.22, -0.55)], 1.2, HOLE);
            line(&[(0.0, 0.0), (0.22, -0.25)], 1.2, HOLE);
        }
        Icon::DragonLair => {
            // Spread wings over a narrow body.
            poly(
                &[(-0.1, -0.2), (-1.0, -0.8), (-0.75, 0.0), (-0.1, 0.35)],
                fill,
            );
            poly(&[(0.1, -0.2), (1.0, -0.8), (0.75, 0.0), (0.1, 0.35)], fill);
            poly(
                &[
                    (0.0, -0.75),
                    (0.18, -0.35),
                    (0.12, 0.95),
                    (-0.12, 0.95),
                    (-0.18, -0.35),
                ],
                fill,
            );
        }
        Icon::Lighthouse => {
            poly(
                &[(-0.4, 0.95), (-0.22, -0.4), (0.22, -0.4), (0.4, 0.95)],
                fill,
            );
            rect(-0.32, -0.55, 0.32, -0.4, fill);
            painter.circle_filled(p(0.0, -0.75), 0.22 * s, fill);
            line(&[(-0.35, -0.75), (-0.85, -0.95)], 1.2, fill);
            line(&[(0.35, -0.75), (0.85, -0.95)], 1.2, fill);
        }
        Icon::Ship => {
            // Longship with a square sail and curled prow.
            poly(
                &[(-0.95, 0.25), (0.95, 0.25), (0.6, 0.75), (-0.6, 0.75)],
                fill,
            );
            line(&[(0.85, 0.3), (1.0, -0.15), (0.85, -0.3)], 1.6, fill);
            line(&[(-0.85, 0.3), (-1.0, -0.15), (-0.85, -0.3)], 1.6, fill);
            line(&[(0.0, 0.25), (0.0, -1.0)], 1.4, fill);
            rect(-0.5, -0.85, 0.5, -0.05, fill);
            line(&[(-0.5, -0.45), (0.5, -0.45)], 1.0, HOLE);
        }
        Icon::Tree => {
            poly(&[(-0.75, 0.45), (0.0, -0.35), (0.75, 0.45)], fill);
            poly(&[(-0.6, -0.05), (0.0, -0.75), (0.6, -0.05)], fill);
            poly(&[(-0.4, -0.45), (0.0, -1.05), (0.4, -0.45)], fill);
            rect(-0.12, 0.45, 0.12, 0.95, fill);
        }
        Icon::Pass => {
            poly(&[(-1.0, 0.8), (-0.35, -0.55), (0.3, 0.8)], fill);
            poly(&[(-0.2, 0.8), (0.4, -0.85), (1.0, 0.8)], fill);
            // The trail through the pass.
            line(&[(0.0, 0.8), (0.15, 0.35), (0.05, 0.0)], 1.2, HOLE);
        }
        Icon::Landmark => {
            poly(&[(0.0, -1.0), (0.7, 0.0), (0.0, 1.0), (-0.7, 0.0)], fill);
            painter.circle_filled(at, 0.2 * s, HOLE);
        }
    }
}

/// A Nordic longhouse: steep roof with crossed gable horns.
fn longhouse(
    poly: &impl Fn(&[(f32, f32)], Color32),
    rect: &impl Fn(f32, f32, f32, f32, Color32),
    line: &impl Fn(&[(f32, f32)], f32, Color32),
    k: f32,
    fill: Color32,
) {
    rect(-0.75 * k, 0.0, 0.75 * k, 0.85, fill);
    poly(&[(-0.95 * k, 0.1), (0.0, -0.7 * k), (0.95 * k, 0.1)], fill);
    line(&[(0.0, -0.7 * k), (-0.35 * k, -1.05 * k)], 1.4, fill);
    line(&[(0.0, -0.7 * k), (0.35 * k, -1.05 * k)], 1.4, fill);
    rect(-0.15 * k, 0.35, 0.15 * k, 0.85, HOLE);
}

/// A name plaque in the game's style: dark leather panel with a thin gold rule.
pub fn label(painter: &Painter, above: Pos2, text: &str, gold: Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(16.0), BONE);
    let size = galley.size() + Vec2::new(18.0, 8.0);
    let rect = egui::Rect::from_center_size(above - Vec2::new(0.0, size.y / 2.0), size);
    painter.rect(
        rect,
        2.0,
        Color32::from_rgba_premultiplied(18, 14, 10, 235),
        Stroke::new(1.0, gold),
        egui::StrokeKind::Inside,
    );
    // Small diamond studs at the ends, like the game's menu ornaments.
    for x in [rect.left(), rect.right()] {
        let c = Pos2::new(x, rect.center().y);
        painter.add(Shape::convex_polygon(
            vec![
                c + Vec2::new(0.0, -4.0),
                c + Vec2::new(4.0, 0.0),
                c + Vec2::new(0.0, 4.0),
                c + Vec2::new(-4.0, 0.0),
            ],
            gold,
            Stroke::NONE,
        ));
    }
    painter.galley(rect.min + Vec2::new(9.0, 4.0), galley, BONE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_map_to_icons() {
        assert_eq!(Icon::for_kind("city"), Icon::City);
        assert_eq!(Icon::for_kind("cave"), Icon::Cave);
        assert_eq!(Icon::for_kind("farm"), Icon::Hut);
        assert_eq!(Icon::for_kind("giant_camp"), Icon::Camp);
        assert_eq!(Icon::for_kind("something_new"), Icon::Landmark);
    }
}
