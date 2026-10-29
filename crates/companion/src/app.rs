use crate::client::{self, Shared, Status};
use crate::config::{CompassConfig, Config};
use crate::history::History;
use crate::quests;
use crate::settings::{self, InstalledFont, Settings};
use crate::widgets::{self, MapView};
use egui::{Color32, RichText};
use protocol::Snapshot;
use std::collections::BTreeMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

pub const GOLD: Color32 = Color32::from_rgb(214, 186, 120);
pub const DIM: Color32 = Color32::from_rgb(140, 132, 118);
pub const RED: Color32 = Color32::from_rgb(200, 60, 50);
pub const GOLDENROD: Color32 = Color32::from_rgb(218, 165, 32);
const LOAD_OK: Color32 = Color32::from_rgb(90, 180, 90);
const LOAD_HEAVY: Color32 = Color32::from_rgb(230, 200, 60);

pub struct CompanionApp {
    shared: Arc<Mutex<Shared>>,
    map: MapView,
    compass: CompassConfig,
    settings: Settings,
    settings_window: SettingsWindow,
    /// Quest picked in the quest list; its markers get a path on the map.
    selected_quest: Option<u32>,
    /// The current character's saved trail and event spots.
    history: History,
}

/// State of the Edit > Settings window.
#[derive(Default)]
struct SettingsWindow {
    open: bool,
    fonts: Option<BTreeMap<String, InstalledFont>>,
    scanning: Option<mpsc::Receiver<BTreeMap<String, InstalledFont>>>,
    status: String,
    /// A reset button was clicked once and waits for the confirming click.
    confirm_reset: Option<Reset>,
}

#[derive(Clone, Copy, PartialEq)]
enum Reset {
    Trail,
    Spots,
}

impl CompanionApp {
    pub fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        if config.window.fullscreen {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        let mut visuals = egui::Visuals::dark();
        // Bronze and gold instead of egui's blue, closer to the game's menus.
        visuals.selection.bg_fill = Color32::from_rgb(78, 58, 30);
        visuals.selection.stroke = egui::Stroke::new(1.0, GOLD);
        visuals.hyperlink_color = GOLD;
        cc.egui_ctx.set_visuals(visuals);
        let settings = Settings::load();
        settings.apply_font(&cc.egui_ctx);
        let shared = client::spawn(config.server.clone(), cc.egui_ctx.clone());
        let map = MapView::new(&cc.egui_ctx, config.map);
        Self {
            shared,
            map,
            compass: config.compass,
            settings,
            settings_window: SettingsWindow::default(),
            selected_quest: None,
            history: History::default(),
        }
    }
}

impl eframe::App for CompanionApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let (status, snapshot) = {
            let s = self.shared.lock().unwrap();
            (s.status.clone(), s.snapshot.clone())
        };
        let live = matches!(status, Status::Connected);
        let snap = snapshot.unwrap_or_default();
        if live {
            self.history.update(&snap);
        }
        let heading = self.compass.heading(&snap.player);
        let colors = widgets::QuestColors::new(&snap);
        let size = self.settings.font_size;

        // F11 switches full screen on and off (there is no title bar in full screen).
        if ui.input(|i| i.key_pressed(egui::Key::F11)) {
            let full = ui.input(|i| i.viewport().fullscreen.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
        }

        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Full screen (F11)").clicked() {
                        let full = ui.input(|i| i.viewport().fullscreen.unwrap_or(false));
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
                        ui.close();
                    }
                    if ui.button("Exit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Settings…").clicked() {
                        self.settings_window.open = true;
                        ui.close();
                    }
                });
            });
        });

        egui::Panel::top("header").show(ui, |ui| {
            ui.add_space(6.0);
            header(ui, &snap, &status, size);
            ui.add_space(6.0);
            widgets::compass(ui, &snap, heading, &colors);
            ui.add_space(6.0);
        });

        egui::Panel::right("quests")
            .default_size(480.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.add_space(6.0);
                ui.heading(RichText::new("Quests").size(size + 4.0).color(GOLD));
                if quests::panel(
                    ui,
                    &snap,
                    &colors,
                    &mut self.selected_quest,
                    &mut self.settings.quest_filter,
                    size,
                ) {
                    let _ = self.settings.save();
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            if !live || !snap.in_game {
                ui.label(RichText::new("Waiting for Skyrim…").color(DIM));
            }
            self.map.keep_centered = self.settings.map_keep_centered;
            if let Some(change) = self.map.show(
                ui,
                &snap,
                heading,
                &colors,
                self.selected_quest,
                &self.history.data,
            ) {
                self.history.change_pin(change);
            }
        });

        self.settings_ui(ui.ctx(), &snap.plugin_version);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.history.save();
    }
}

impl CompanionApp {
    fn settings_ui(&mut self, ctx: &egui::Context, plugin_version: &str) {
        let w = &mut self.settings_window;
        if !w.open {
            return;
        }
        // Scan the installed fonts once, in the background, the first time the window opens.
        if w.fonts.is_none() && w.scanning.is_none() {
            w.scanning = Some(settings::scan_fonts());
        }
        if let Some(rx) = &w.scanning {
            if let Ok(fonts) = rx.try_recv() {
                w.fonts = Some(fonts);
                w.scanning = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }

        let before = self.settings.clone();
        let mut open = w.open;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                egui::Grid::new("settings-grid")
                    .num_columns(2)
                    .spacing([16.0, 10.0])
                    .show(ui, |ui| {
                        ui.label("Font size");
                        ui.add(
                            egui::Slider::new(&mut self.settings.font_size, 10.0..=48.0)
                                .step_by(1.0)
                                .suffix(" pt"),
                        );
                        ui.end_row();

                        ui.label("Font family");
                        let current = self
                            .settings
                            .font_family
                            .clone()
                            .unwrap_or_else(|| "Built-in".into());
                        egui::ComboBox::from_id_salt("font-family")
                            .width(260.0)
                            .selected_text(current)
                            .height(400.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        self.settings.font_family.is_none(),
                                        "Built-in",
                                    )
                                    .clicked()
                                {
                                    self.settings.font_family = None;
                                    self.settings.font_file = None;
                                    self.settings.font_index = 0;
                                }
                                match &w.fonts {
                                    None => {
                                        ui.label(
                                            RichText::new("Looking for installed fonts…")
                                                .color(DIM),
                                        );
                                    }
                                    Some(fonts) => {
                                        for (name, font) in fonts {
                                            let chosen =
                                                self.settings.font_family.as_deref() == Some(name);
                                            if ui.selectable_label(chosen, name).clicked() {
                                                self.settings.font_family = Some(name.clone());
                                                self.settings.font_file = Some(font.file.clone());
                                                self.settings.font_index = font.index;
                                            }
                                        }
                                    }
                                }
                            });
                        ui.end_row();

                        ui.label("Map");
                        ui.checkbox(
                            &mut self.settings.map_keep_centered,
                            "Keep the player centred (the map moves with them)",
                        )
                        .on_hover_text(
                            "With Follow player on. Off: the map only moves when the player \
                             leaves the visible area, so you can pan around freely.",
                        );
                        ui.end_row();
                    });

                ui.separator();
                ui.label(RichText::new("Preview").color(DIM));
                ui.label(
                    RichText::new("» The Way of the Voice — Speak to the Greybeards")
                        .size(self.settings.font_size)
                        .color(GOLD),
                );
                ui.separator();
                history_ui(ui, &mut self.history, &mut w.confirm_reset);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Reset to defaults").clicked() {
                        self.settings = Settings::default();
                    }
                    ui.label(RichText::new(&w.status).small().color(DIM));
                });
                ui.separator();
                let plugin = if plugin_version.is_empty() {
                    "not connected".to_string()
                } else {
                    plugin_version.to_string()
                };
                ui.label(
                    RichText::new(format!(
                        "Skyrim Companion {} \u{B7} SKSE plugin {plugin}",
                        env!("COMPANION_VERSION")
                    ))
                    .small()
                    .color(DIM),
                );
            });
        w.open = open;

        // Settings apply immediately and are saved for the next run.
        if self.settings != before {
            if self.settings.font_file != before.font_file
                || self.settings.font_index != before.font_index
            {
                self.settings.apply_font(ctx);
            }
            w.status = match self.settings.save() {
                Ok(()) => Settings::path()
                    .map(|p| format!("Saved to {}", p.display()))
                    .unwrap_or_default(),
                Err(e) => format!("Not saved: {e}"),
            };
        }
    }
}

/// Settings section for the saved map trail and event spots of the current character.
fn history_ui(ui: &mut egui::Ui, history: &mut History, confirm: &mut Option<Reset>) {
    let d = &history.data;
    if d.character.is_empty() {
        ui.label(RichText::new("Map history: load a character in the game first").color(DIM));
        return;
    }
    ui.label(format!("Map history for {}", d.character));
    ui.label(
        RichText::new(format!(
            "{} trail points, {} event points",
            d.trail.len(),
            d.spots.len()
        ))
        .small()
        .color(DIM),
    );
    if let Some(s) = &history.load_stats {
        let mut text = format!(
            "Loaded {} KB in {} ms (read {} ms, parse {} ms)",
            s.bytes / 1024,
            s.total().as_millis(),
            s.read.as_millis(),
            s.parse.as_millis()
        );
        if s.trimmed > 0 {
            text += &format!(
                "; slow, so the oldest {} trail points were dropped",
                s.trimmed
            );
        }
        ui.label(RichText::new(text).small().color(DIM));
    }
    let (trail_empty, spots_empty) = (d.trail.is_empty(), d.spots.is_empty());
    ui.horizontal(|ui| {
        for (which, label, empty) in [
            (Reset::Trail, "Reset trail", trail_empty),
            (Reset::Spots, "Reset event points", spots_empty),
        ] {
            let armed = *confirm == Some(which);
            let text = if armed { "Click again to reset" } else { label };
            let button = egui::Button::new(if armed {
                RichText::new(text).color(RED)
            } else {
                RichText::new(text)
            });
            if ui.add_enabled(!empty, button).clicked() {
                if armed {
                    match which {
                        Reset::Trail => history.reset_trail(),
                        Reset::Spots => history.reset_spots(),
                    }
                    *confirm = None;
                } else {
                    *confirm = Some(which);
                }
            }
        }
    });
    if let Some(e) = &history.error {
        ui.label(RichText::new(format!("Not saved: {e}")).small().color(RED));
    }
}

fn header(ui: &mut egui::Ui, snap: &Snapshot, status: &Status, size: f32) {
    let small = size * 0.75;
    // The right-hand block is laid out first; the left side gives way when the window is narrow,
    // so the two never overlap.
    egui::containers::Sides::new().shrink_left().show(
        ui,
        |ui| {
            // When space runs out, cut text off with "…" rather than wrapping it.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            // Time of day.
            ui.vertical(|ui| {
                ui.label(RichText::new(snap.time.clock()).size(size).color(GOLD));
                ui.label(
                    RichText::new(format!("{} · {}", snap.time.period(), snap.time.date()))
                        .size(small)
                        .color(DIM),
                );
            });
            ui.separator();

            // Level and XP.
            ui.vertical(|ui| {
                let p = &snap.player;
                ui.label(
                    RichText::new(format!("Level {}", p.level))
                        .size(size)
                        .color(GOLD),
                );
                let remaining = (p.xp_next - p.xp).max(0.0);
                ui.add(
                    egui::ProgressBar::new(p.xp_fraction())
                        .desired_width(size * 13.0)
                        .desired_height(small * 1.4)
                        .text(
                            RichText::new(format!(
                                "{:.0} / {:.0} XP  ({remaining:.0} to next level)",
                                p.xp, p.xp_next
                            ))
                            .size(small),
                        ),
                );
            });
            ui.separator();

            // Location.
            ui.vertical(|ui| {
                let p = &snap.player;
                let place = if p.location.is_empty() {
                    &p.cell
                } else {
                    &p.location
                };
                ui.label(RichText::new(place).size(size));
                ui.label(
                    RichText::new(if p.is_interior {
                        "Interior"
                    } else {
                        p.worldspace.as_str()
                    })
                    .size(small)
                    .color(DIM),
                );
            });
        },
        |ui| {
            // Wanted and connection status, with gold and carry weight beneath, right-aligned.
            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                status_line(ui, snap, status, size);
                purse_line(ui, &snap.player, small);
            });
        },
    );
}

/// "WANTED · 40 gold   • Live" (or "Not wanted"), as one right-aligned line.
fn status_line(ui: &mut egui::Ui, snap: &Snapshot, status: &Status, size: f32) {
    let mut line = egui::text::LayoutJob::default();
    let wanted = snap.crime.is_wanted();
    if wanted {
        line.append(
            &format!("WANTED · {} gold", snap.crime.total()),
            0.0,
            text_format(size, RED),
        );
    } else {
        line.append("Not wanted", 0.0, text_format(size, DIM));
    }
    let (text, color) = match status {
        Status::Connected => ("• Live", Color32::from_rgb(90, 180, 90)),
        Status::Connecting => ("• Connecting", DIM),
        Status::Disconnected(_) => ("• Offline", RED),
    };
    line.append(text, size, text_format(size, color));

    let mut hover = Vec::new();
    if wanted {
        hover.extend(
            snap.crime
                .bounties
                .iter()
                .filter(|b| b.total() > 0)
                .map(|b| format!("{}: {} gold", b.faction, b.total())),
        );
    }
    if let Status::Disconnected(why) = status {
        hover.push(why.clone());
    }
    let r = ui.label(line);
    if !hover.is_empty() {
        r.on_hover_text(hover.join("\n"));
    }
}

/// "Gold 1234   Weight 216 / 300": gold in goldenrod, and carry weight coloured by load:
/// green under 70 %, yellow up to capacity, red when encumbered (over capacity).
fn purse_line(ui: &mut egui::Ui, p: &protocol::Player, size: f32) {
    let load = p.load_fraction();
    let weight_color = if p.carry_weight > p.max_carry_weight {
        RED
    } else if load >= 0.70 {
        LOAD_HEAVY
    } else {
        LOAD_OK
    };
    let mut line = egui::text::LayoutJob::default();
    line.append("Gold ", 0.0, text_format(size, DIM));
    line.append(&p.gold.to_string(), 0.0, text_format(size, GOLDENROD));
    line.append("Weight ", size, text_format(size, DIM));
    line.append(
        &format!("{:.0} / {:.0}", p.carry_weight, p.max_carry_weight),
        0.0,
        text_format(size, weight_color),
    );
    ui.label(line)
        .on_hover_text(format!("{:.0} % of carry capacity", load * 100.0));
}

fn text_format(size: f32, color: Color32) -> egui::TextFormat {
    egui::TextFormat::simple(egui::FontId::proportional(size), color)
}
