use crate::app::{DIM, GOLD, RED};
use crate::config::MapConfig;
use crate::history;
use crate::icons;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use protocol::{Marker, MarkerKind, Snapshot, Vec3};
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

/// Degrees of view either side of the facing direction, like the in-game compass.
const COMPASS_HALF_FOV: f32 = PI / 2.0;

/// Game convention: +Y is north, +X is east, heading 0 = north, clockwise.
fn bearing(from: Vec3, to: Vec3) -> f32 {
    (to.x - from.x).atan2(to.y - from.y)
}

fn wrap_pi(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

/// `heading` is the direction the compass centres on (see `CompassConfig`).
pub fn compass(ui: &mut egui::Ui, snap: &Snapshot, heading: f32, colors: &QuestColors) {
    let width = ui.available_width().min(900.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::hover());
    let rect = Rect::from_center_size(rect.center(), Vec2::new(width, rect.height()));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_black_alpha(120));

    let to_x = |angle: f32| -> Option<f32> {
        let rel = wrap_pi(angle - heading);
        (rel.abs() <= COMPASS_HALF_FOV)
            .then(|| rect.center().x + rel / COMPASS_HALF_FOV * rect.width() / 2.0)
    };

    for i in 0..24 {
        let angle = i as f32 * TAU / 24.0;
        let Some(x) = to_x(angle) else { continue };
        let label = match i {
            0 => Some("N"),
            6 => Some("E"),
            12 => Some("S"),
            18 => Some("W"),
            _ => None,
        };
        match label {
            Some(l) => {
                painter.text(
                    Pos2::new(x, rect.center().y),
                    Align2::CENTER_CENTER,
                    l,
                    FontId::proportional(20.0),
                    GOLD,
                );
            }
            None => {
                let h = if i % 3 == 0 { 10.0 } else { 5.0 };
                painter.line_segment(
                    [
                        Pos2::new(x, rect.center().y - h),
                        Pos2::new(x, rect.center().y + h),
                    ],
                    Stroke::new(1.0, DIM),
                );
            }
        }
    }

    for m in &snap.markers {
        let Some(x) = to_x(bearing(snap.player.position, m.position)) else {
            continue;
        };
        let color = colors.marker(m);
        let top = rect.top() + 4.0;
        painter.add(Shape::convex_polygon(
            vec![
                Pos2::new(x, top + 10.0),
                Pos2::new(x - 6.0, top),
                Pos2::new(x + 6.0, top),
            ],
            color,
            Stroke::NONE,
        ));
    }

    painter.line_segment(
        [
            Pos2::new(rect.center().x, rect.top()),
            Pos2::new(rect.center().x, rect.bottom()),
        ],
        Stroke::new(1.5, Color32::WHITE),
    );
}

/// One colour per quest that has markers, shared by the compass, the map and the quest list.
/// The tracked quest gets gold, like the in-game compass.
pub struct QuestColors(HashMap<u32, Color32>);

impl QuestColors {
    /// Blue and red are left out: they mean the custom marker and the player arrow.
    const PALETTE: [Color32; 7] = [
        GOLD,
        Color32::from_rgb(90, 200, 180),
        Color32::from_rgb(220, 120, 210),
        Color32::from_rgb(240, 150, 60),
        Color32::from_rgb(150, 210, 90),
        Color32::from_rgb(170, 150, 250),
        Color32::from_rgb(240, 230, 140),
    ];

    pub fn new(snap: &Snapshot) -> Self {
        let mut ids: Vec<u32> = snap.markers.iter().filter_map(|m| m.quest).collect();
        ids.sort_by_key(|id| {
            let tracked = snap.quests.iter().any(|q| q.form_id == *id && q.tracked);
            (!tracked, *id)
        });
        ids.dedup();
        Self(
            ids.into_iter()
                .enumerate()
                .map(|(i, id)| (id, Self::PALETTE[i % Self::PALETTE.len()]))
                .collect(),
        )
    }

    /// Colour of a quest's markers, or None if it has none.
    pub fn quest(&self, form_id: u32) -> Option<Color32> {
        self.0.get(&form_id).copied()
    }

    pub fn marker(&self, m: &Marker) -> Color32 {
        match (m.kind, m.quest.and_then(|q| self.quest(q))) {
            (_, Some(c)) => c,
            (MarkerKind::QuestTarget, None) => GOLD,
            (MarkerKind::Location, None) => Color32::LIGHT_GRAY,
            (MarkerKind::Custom, None) => Color32::from_rgb(90, 160, 220),
        }
    }
}

pub struct MapView {
    cfg: MapConfig,
    texture: Option<egui::TextureHandle>,
    image_error: Option<String>,
    /// Map-space (0..1) point at the view centre.
    center: Vec2,
    /// Set once the view has been centred on the player after start-up.
    centered: bool,
    /// Where the map was right-clicked (world x, y) and the player marker under the mouse, if any.
    menu: Option<(f32, f32, Option<usize>)>,
    /// The map marker window, while adding or editing one.
    editor: Option<PinEditor>,
}

struct PinEditor {
    /// None when adding a new marker.
    index: Option<usize>,
    pin: history::Pin,
}

impl MapView {
    pub fn new(ctx: &egui::Context, cfg: MapConfig) -> Self {
        let (texture, image_error) = match load_texture(ctx, &cfg.image) {
            Ok(t) => (Some(t), None),
            Err(e) => (None, Some(e)),
        };
        Self {
            cfg,
            texture,
            image_error,
            center: Vec2::splat(0.5),
            centered: false,
            menu: None,
            editor: None,
        }
    }

    /// World coordinates to normalized map coordinates (0,0 = top-left).
    fn to_uv(&self, p: Vec3) -> Vec2 {
        let c = &self.cfg;
        Vec2::new(
            (p.x - c.world_min_x) / (c.world_max_x - c.world_min_x),
            1.0 - (p.y - c.world_min_y) / (c.world_max_y - c.world_min_y),
        )
    }

    /// `selected` is the quest whose markers get an animated path from the player;
    /// `history` holds the character's saved trail, event spots and own markers.
    /// Returns an edit to the player's markers when one was made.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        snap: &Snapshot,
        heading: f32,
        colors: &QuestColors,
        selected: Option<u32>,
        history: &history::Data,
    ) -> Option<history::PinChange> {
        let on_map = !snap.player.is_interior && snap.player.worldspace == history::WORLDSPACE;
        let mut change = None;

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.cfg.follow_player, "Follow player");
            ui.add(
                egui::Slider::new(&mut self.cfg.zoom, 1.0..=12.0)
                    .logarithmic(true)
                    .text("Zoom"),
            );
            if let Some(e) = &self.image_error {
                ui.label(
                    egui::RichText::new(format!("No map image ({e})"))
                        .small()
                        .color(DIM),
                );
            }
        });

        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(24, 22, 20));

        // Scroll to zoom, drag to pan.
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if response.hovered() && scroll != 0.0 {
            self.cfg.zoom = (self.cfg.zoom * (1.0 + scroll * 0.002)).clamp(1.0, 12.0);
        }
        // At zoom 1 the map covers the whole view (no empty border), keeping the world's aspect ratio.
        let c = &self.cfg;
        let aspect = (c.world_max_x - c.world_min_x) / (c.world_max_y - c.world_min_y);
        let fit = (rect.width() / aspect).max(rect.height());
        let size = Vec2::new(fit * aspect, fit) * self.cfg.zoom;
        if response.dragged() {
            self.center -= response.drag_delta() / size;
        }
        // Follow: centre on the player at start-up, then only when they leave the visible area,
        // so manual panning sticks while the player is in view.
        if self.cfg.follow_player && on_map {
            let player = self.to_uv(snap.player.position);
            let on_screen = rect.center() + (player - self.center) * size;
            if !self.centered || !rect.shrink(12.0).contains(on_screen) {
                self.center = player;
                self.centered = true;
            }
        }
        // Keep the view inside the map so no empty border shows.
        let half = rect.size() / size / 2.0;
        let clamp = |v: f32, h: f32| if h >= 0.5 { 0.5 } else { v.clamp(h, 1.0 - h) };
        self.center = Vec2::new(clamp(self.center.x, half.x), clamp(self.center.y, half.y));
        let to_screen = |uv: Vec2| rect.center() + (uv - self.center) * size;

        let map_rect = Rect::from_min_max(to_screen(Vec2::ZERO), to_screen(Vec2::splat(1.0)));
        match &self.texture {
            Some(t) => {
                painter.image(
                    t.id(),
                    map_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => draw_cell_grid(&painter, map_rect, &self.cfg),
        }

        let to_screen_xy = |x: f32, y: f32| to_screen(self.to_uv(Vec3 { x, y, z: 0.0 }));
        draw_trail(&painter, rect, &history.trail, to_screen_xy);

        // Found locations. A name shows while the mouse is over the icon, or while the player
        // is within PLACE_NEARBY_UNITS of it (outside in Tamriel).
        let hover = response.hover_pos();
        let mut labels: Vec<(Pos2, &str)> = Vec::new();
        for place in &snap.places {
            let pos = to_screen(self.to_uv(place.position));
            if !rect.expand(PLACE_ICON_RADIUS * 2.0).contains(pos) {
                continue;
            }
            icons::draw(
                &painter,
                icons::Icon::for_kind(&place.kind),
                pos,
                PLACE_ICON_RADIUS,
                !place.discovered,
            );
            let hovered = hover.is_some_and(|h| h.distance(pos) <= PLACE_ICON_RADIUS * 1.3);
            let nearby = on_map
                && dist2(snap.player.position, place.position)
                    <= PLACE_NEARBY_UNITS * PLACE_NEARBY_UNITS;
            if hovered || nearby {
                labels.push((pos, &place.name));
            }
        }

        // Where notable things happened: small gold points, red where something was killed.
        // Details show on hover, or while the player is within 5 m.
        let mut spot_labels: Vec<(Pos2, String, Color32)> = Vec::new();
        for spot in &history.spots {
            let pos = to_screen(self.to_uv(spot.position));
            if !rect.expand(SPOT_RADIUS_PX * 2.0).contains(pos) {
                continue;
            }
            let color = if spot.has_kill() { KILL_RED } else { GOLD };
            painter.circle(pos, SPOT_RADIUS_PX, color, Stroke::new(1.0, Color32::BLACK));
            let hovered = hover.is_some_and(|h| h.distance(pos) <= SPOT_RADIUS_PX + 4.0);
            let nearby = on_map
                && dist2(snap.player.position, spot.position)
                    <= history::SPOT_RADIUS * history::SPOT_RADIUS;
            if hovered || nearby {
                spot_labels.push((pos, spot.summary(), color));
            }
        }

        // The player's own markers: an icon or a point, with their note on hover or within 25 m.
        let mut pin_labels: Vec<(Pos2, &str, f32)> = Vec::new();
        let mut hovered_pin = None;
        for (i, pin) in history.pins.iter().enumerate() {
            let pos = to_screen(self.to_uv(pin.position()));
            if !rect.expand(PLACE_ICON_RADIUS * 2.0).contains(pos) {
                continue;
            }
            let r = draw_pin(&painter, pin, pos);
            let hovered = hover.is_some_and(|h| h.distance(pos) <= r + 3.0);
            if hovered {
                hovered_pin = Some(i);
            }
            let nearby = on_map
                && dist2(snap.player.position, pin.position())
                    <= PLACE_NEARBY_UNITS * PLACE_NEARBY_UNITS;
            if (hovered || nearby) && !pin.text.is_empty() {
                pin_labels.push((pos, &pin.text, r));
            }
        }

        // Right-click: add a marker there, or edit / delete the one under the mouse.
        if let Some(at) = response
            .interact_pointer_pos()
            .filter(|_| response.secondary_clicked())
        {
            let uv = self.center + (at - rect.center()) / size;
            let c = &self.cfg;
            let x = c.world_min_x + uv.x * (c.world_max_x - c.world_min_x);
            let y = c.world_min_y + (1.0 - uv.y) * (c.world_max_y - c.world_min_y);
            self.menu = Some((x, y, hovered_pin));
        }
        response.context_menu(|ui| {
            let Some((x, y, pin)) = self.menu else {
                ui.close();
                return;
            };
            match pin.and_then(|i| history.pins.get(i).map(|p| (i, p))) {
                Some((i, existing)) => {
                    if ui.button("Edit marker\u{2026}").clicked() {
                        self.editor = Some(PinEditor {
                            index: Some(i),
                            pin: existing.clone(),
                        });
                        ui.close();
                    }
                    if ui.button("Delete marker").clicked() {
                        change = Some(history::PinChange::Remove(i));
                        ui.close();
                    }
                }
                None => {
                    if ui.button("Add marker here\u{2026}").clicked() {
                        self.editor = Some(PinEditor {
                            index: None,
                            pin: history::Pin {
                                x,
                                y,
                                ..Default::default()
                            },
                        });
                        ui.close();
                    }
                }
            }
        });

        // Markers are in the player's current space, so they only belong on the map outside.
        if on_map {
            let player = to_screen(self.to_uv(snap.player.position));
            if let Some(quest) = selected {
                let time = ui.input(|i| i.time) as f32;
                for m in snap.markers.iter().filter(|m| m.quest == Some(quest)) {
                    let target = to_screen(self.to_uv(m.position));
                    draw_quest_path(&painter, rect, player, target, colors.marker(m), time);
                    if !rect.contains(target) {
                        draw_edge_arrow(
                            &painter,
                            rect,
                            target,
                            colors.marker(m),
                            dist2(snap.player.position, m.position).sqrt(),
                        );
                    }
                }
                if !snap.markers.iter().any(|m| m.quest == Some(quest)) {
                    painter.text(
                        rect.left_top() + Vec2::new(12.0, 12.0),
                        Align2::LEFT_TOP,
                        "This quest has no marker on the map right now",
                        FontId::proportional(16.0),
                        DIM,
                    );
                }
                ui.ctx().request_repaint();
            }
            for m in &snap.markers {
                let pos = to_screen(self.to_uv(m.position));
                painter.circle(pos, 6.0, colors.marker(m), Stroke::new(1.0, Color32::BLACK));
                painter.text(
                    pos + Vec2::new(9.0, 0.0),
                    Align2::LEFT_CENTER,
                    &m.name,
                    FontId::proportional(13.0),
                    Color32::WHITE,
                );
            }
        }

        for (pos, text, color) in &spot_labels {
            icons::label(
                &painter,
                *pos - Vec2::new(0.0, SPOT_RADIUS_PX * 2.0),
                text,
                *color,
            );
        }
        for (pos, text, r) in &pin_labels {
            icons::label(&painter, *pos - Vec2::new(0.0, r * 1.5), text, PIN_COLOR);
        }
        for (pos, name) in labels {
            icons::label(
                &painter,
                pos - Vec2::new(0.0, PLACE_ICON_RADIUS * 1.5),
                name,
                GOLD,
            );
        }

        if on_map {
            draw_player_arrow(
                &painter,
                to_screen(self.to_uv(snap.player.position)),
                heading,
            );
        } else if snap.in_game {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Interior / other worldspace",
                FontId::proportional(18.0),
                DIM,
            );
        }

        if let Some(edit) = self.pin_editor(ui.ctx()) {
            change = Some(edit);
        }
        change
    }

    /// The map marker window: a note and an icon (or a plain point).
    fn pin_editor(&mut self, ctx: &egui::Context) -> Option<history::PinChange> {
        let editor = self.editor.as_mut()?;
        let mut open = true;
        let mut result = None;
        let mut done = false;
        let title = if editor.index.is_some() {
            "Edit map marker"
        } else {
            "New map marker"
        };
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label("Text");
                ui.add(
                    egui::TextEdit::multiline(&mut editor.pin.text)
                        .hint_text("e.g. Come back with a pickaxe")
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);
                ui.label("Icon");
                let choices: Vec<Option<icons::Icon>> = std::iter::once(None)
                    .chain(icons::Icon::ALL.map(Some))
                    .collect();
                ui.horizontal_wrapped(|ui| {
                    for choice in choices {
                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
                        let chosen = editor.pin.icon == choice;
                        if chosen || resp.hovered() {
                            let fill = if chosen {
                                ui.visuals().selection.bg_fill
                            } else {
                                ui.visuals().widgets.hovered.bg_fill
                            };
                            ui.painter().rect_filled(r, 4.0, fill);
                        }
                        let preview = history::Pin {
                            icon: choice,
                            ..Default::default()
                        };
                        draw_pin(ui.painter(), &preview, r.center());
                        let name = choice.map_or("Point", icons::Icon::label);
                        if resp.on_hover_text(name).clicked() {
                            editor.pin.icon = choice;
                        }
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        result = Some(match editor.index {
                            Some(i) => history::PinChange::Update(i, editor.pin.clone()),
                            None => history::PinChange::Add(editor.pin.clone()),
                        });
                        done = true;
                    }
                    if ui.button("Cancel").clicked() {
                        done = true;
                    }
                    if let Some(i) = editor.index {
                        if ui
                            .button(egui::RichText::new("Delete").color(RED))
                            .clicked()
                        {
                            result = Some(history::PinChange::Remove(i));
                            done = true;
                        }
                    }
                });
            });
        if done || !open {
            self.editor = None;
        }
        result
    }
}

/// Player markers are outlined in this colour so they stand apart from the game's places.
const PIN_COLOR: Color32 = Color32::from_rgb(110, 200, 235);

/// Draws a player marker: a map icon with a coloured ring, or a small point. Returns its radius.
fn draw_pin(painter: &egui::Painter, pin: &history::Pin, at: Pos2) -> f32 {
    match pin.icon {
        Some(icon) => {
            icons::draw(painter, icon, at, PLACE_ICON_RADIUS, false);
            painter.circle_stroke(at, PLACE_ICON_RADIUS * 1.3, Stroke::new(2.0, PIN_COLOR));
            PLACE_ICON_RADIUS * 1.3
        }
        None => {
            painter.circle(at, 5.0, PIN_COLOR, Stroke::new(1.5, Color32::BLACK));
            5.0
        }
    }
}

/// Radius of an event spot on the map, in points.
const SPOT_RADIUS_PX: f32 = 3.0;
const KILL_RED: Color32 = Color32::from_rgb(210, 40, 30);
/// Trail colours: the newest movement is dark blue, fading to very light blue for the oldest.
const TRAIL_NEW: [f32; 3] = [16.0, 48.0, 170.0];
const TRAIL_OLD: [f32; 3] = [200.0, 225.0, 255.0];

/// The saved trail, newest dark and oldest light. Points are grouped into a few colour bands so
/// a long trail is a handful of line shapes, and points off screen or too close together are skipped.
fn draw_trail(
    painter: &egui::Painter,
    view: Rect,
    trail: &[history::TrailPoint],
    to_screen: impl Fn(f32, f32) -> Pos2,
) {
    const BANDS: usize = 48;
    let n = trail.len();
    if n < 2 {
        return;
    }
    let color = |band: usize| {
        let t = band as f32 / (BANDS - 1) as f32; // 0 = oldest, 1 = newest
        let c = |i: usize| (TRAIL_OLD[i] + (TRAIL_NEW[i] - TRAIL_OLD[i]) * t) as u8;
        Color32::from_rgb(c(0), c(1), c(2))
    };
    let area = view.expand(40.0);
    let mut run: Vec<Pos2> = Vec::new();
    let mut run_band = 0;
    let flush = |run: &mut Vec<Pos2>, band: usize| {
        if run.len() > 1 {
            painter.add(Shape::line(
                std::mem::take(run),
                Stroke::new(2.5, color(band)),
            ));
        }
        run.clear();
    };
    for (i, p) in trail.iter().enumerate() {
        let band = i * BANDS / n;
        let pos = to_screen(p.x, p.y);
        if p.gap || !area.contains(pos) {
            flush(&mut run, run_band);
            if area.contains(pos) {
                run.push(pos);
            }
            run_band = band;
            continue;
        }
        if band != run_band {
            // Start the next band where this one ends so the line stays continuous.
            let last = run.last().copied();
            flush(&mut run, run_band);
            run.extend(last);
            run_band = band;
        }
        if run.last().is_none_or(|l| l.distance(pos) >= 1.5) || i == n - 1 {
            run.push(pos);
        }
    }
    flush(&mut run, run_band);
}

/// Radius of a location icon on the map, in points.
const PLACE_ICON_RADIUS: f32 = 10.0;
/// 25 metres in game units (one unit is about 1.43 cm).
const PLACE_NEARBY_UNITS: f32 = 25.0 / 0.0143;

/// Marching dashes from the player to a quest target, with a pulsing ring on the target.
fn draw_quest_path(
    painter: &egui::Painter,
    view: Rect,
    from: Pos2,
    to: Pos2,
    color: Color32,
    time: f32,
) {
    const DASH: f32 = 14.0;
    const GAP: f32 = 9.0;
    let delta = to - from;
    let len = delta.length();
    if len > 1.0 {
        let dir = delta / len;
        // Dashes move towards the target.
        let mut d = (time * 40.0) % (DASH + GAP) - (DASH + GAP);
        while d < len {
            let a = from + dir * d.max(0.0);
            let b = from + dir * (d + DASH).min(len);
            if d + DASH > 0.0 && view.expand(DASH).contains(a) | view.expand(DASH).contains(b) {
                painter.line_segment([a, b], Stroke::new(6.0, Color32::from_black_alpha(160)));
                painter.line_segment([a, b], Stroke::new(3.5, color));
            }
            d += DASH + GAP;
        }
    }
    let phase = (time * 1.2).fract();
    painter.circle_stroke(
        to,
        8.0 + 16.0 * phase,
        Stroke::new(3.0, color.gamma_multiply(1.0 - phase)),
    );
    painter.circle_stroke(to, 10.0, Stroke::new(2.0, color));
}

/// Arrow on the view border pointing at an off-screen target, with its distance.
fn draw_edge_arrow(painter: &egui::Painter, view: Rect, to: Pos2, color: Color32, world_dist: f32) {
    let inner = view.shrink(28.0);
    let c = inner.center();
    let dir = (to - c).normalized();
    // Scale the direction until it hits the inner border.
    let tx = if dir.x.abs() > 1e-4 {
        (inner.width() / 2.0) / dir.x.abs()
    } else {
        f32::INFINITY
    };
    let ty = if dir.y.abs() > 1e-4 {
        (inner.height() / 2.0) / dir.y.abs()
    } else {
        f32::INFINITY
    };
    let at = c + dir * tx.min(ty);
    let perp = Vec2::new(-dir.y, dir.x);
    painter.add(Shape::convex_polygon(
        vec![
            at + dir * 14.0,
            at - dir * 8.0 + perp * 10.0,
            at - dir * 8.0 - perp * 10.0,
        ],
        color,
        Stroke::new(1.5, Color32::BLACK),
    ));
    // One game unit is about 1.43 cm.
    let metres = world_dist * 0.0143;
    let text = if metres >= 1000.0 {
        format!("{:.1} km", metres / 1000.0)
    } else {
        format!("{metres:.0} m")
    };
    painter.text(
        at - dir * 24.0,
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(14.0),
        Color32::WHITE,
    );
}

fn dist2(a: Vec3, b: Vec3) -> f32 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

fn draw_player_arrow(painter: &egui::Painter, at: Pos2, heading: f32) {
    // Screen y points down, so north (heading 0) is -y.
    let dir = Vec2::new(heading.sin(), -heading.cos());
    let perp = Vec2::new(-dir.y, dir.x);
    let pts = vec![
        at + dir * 12.0,
        at - dir * 7.0 + perp * 7.0,
        at - dir * 3.0,
        at - dir * 7.0 - perp * 7.0,
    ];
    painter.add(Shape::convex_polygon(
        pts,
        RED,
        Stroke::new(1.5, Color32::WHITE),
    ));
}

fn draw_cell_grid(painter: &egui::Painter, map_rect: Rect, cfg: &MapConfig) {
    painter.rect_stroke(
        map_rect,
        0.0,
        Stroke::new(1.0, DIM),
        egui::StrokeKind::Inside,
    );
    let cells_x = ((cfg.world_max_x - cfg.world_min_x) / 4096.0)
        .round()
        .max(1.0);
    let cells_y = ((cfg.world_max_y - cfg.world_min_y) / 4096.0)
        .round()
        .max(1.0);
    let stroke = Stroke::new(1.0, Color32::from_white_alpha(12));
    let step = 8.0;
    let mut i = 0.0;
    while i <= cells_x {
        let x = map_rect.left() + map_rect.width() * i / cells_x;
        painter.line_segment(
            [
                Pos2::new(x, map_rect.top()),
                Pos2::new(x, map_rect.bottom()),
            ],
            stroke,
        );
        i += step;
    }
    let mut j = 0.0;
    while j <= cells_y {
        let y = map_rect.top() + map_rect.height() * j / cells_y;
        painter.line_segment(
            [
                Pos2::new(map_rect.left(), y),
                Pos2::new(map_rect.right(), y),
            ],
            stroke,
        );
        j += step;
    }
}

fn load_texture(ctx: &egui::Context, path: &str) -> Result<egui::TextureHandle, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let img = image::load_from_memory(&bytes)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Ok(ctx.load_texture("skyrim-map", color, egui::TextureOptions::LINEAR))
}
