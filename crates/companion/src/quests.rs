//! The quest panel: filter buttons (Tracked / Current / Completed / Unknown) over a table of
//! quests with their type, description and progress. Clicking a quest selects it for the map path.

use crate::app::{DIM, GOLD, RED};
use crate::widgets::QuestColors;
use egui::{Color32, RichText};
use protocol::{ObjectiveState, Quest, QuestKind, QuestStatus, Snapshot};

/// Draws the panel. Returns true when the filter changed (so settings can be saved).
pub fn panel(
    ui: &mut egui::Ui,
    snap: &Snapshot,
    colors: &QuestColors,
    selected: &mut Option<u32>,
    filter: &mut Vec<QuestStatus>,
    size: f32,
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for status in QuestStatus::ALL {
            let count = snap
                .quests
                .iter()
                .filter(|q| self::status(q) == status)
                .count();
            let on = filter.contains(&status);
            let text = RichText::new(format!("{} {count}", status.label()))
                .size(size * 0.7)
                .color(if on { GOLD } else { DIM });
            if ui.selectable_label(on, text).clicked() {
                if on {
                    filter.retain(|s| *s != status);
                } else {
                    filter.push(status);
                    filter.sort();
                }
                changed = true;
            }
        }
    });
    ui.separator();

    let mut list: Vec<&Quest> = snap
        .quests
        .iter()
        .filter(|q| filter.contains(&status(q)))
        .collect();
    list.sort_by(|a, b| {
        status(a)
            .cmp(&status(b))
            .then(a.kind.cmp(&b.kind))
            .then(a.name.is_empty().cmp(&b.name.is_empty()))
            .then(a.name.cmp(&b.name))
    });
    if list.is_empty() {
        ui.label(
            RichText::new("No quests match the filter")
                .size(size * 0.85)
                .color(DIM),
        );
        return changed;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        let gap = ui.spacing().item_spacing.x;
        let type_w = size * 4.4;
        let done_w = size * 3.2;
        // Leave room for the scroll bar on the right.
        let bar = ui.spacing().scroll.bar_width + ui.spacing().scroll.bar_outer_margin + gap;
        let desc_w = (ui.available_width() - type_w - done_w - gap * 2.0 - bar).max(size * 6.0);
        let widths = [type_w, desc_w, done_w];
        let head =
            |t: &str| egui::Label::new(RichText::new(t).size(size * 0.65).color(DIM)).truncate();
        ui.horizontal_top(|ui| {
            cell(ui, type_w, |ui| ui.add(head("Quest Type")));
            cell(ui, desc_w, |ui| ui.add(head("Description")));
            cell(ui, done_w, |ui| ui.add(head("Complete")));
        });
        // Each row is laid out on its own, so a long wrapped description makes only its row taller.
        for (i, q) in list.into_iter().enumerate() {
            let is_selected = *selected == Some(q.form_id);
            let fill = if is_selected {
                ui.visuals().selection.bg_fill
            } else if i % 2 == 0 {
                ui.visuals().faint_bg_color
            } else {
                Color32::TRANSPARENT
            };
            let clicked = egui::Frame::new()
                .fill(fill)
                .inner_margin(egui::Margin::symmetric(0, 4))
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| row(ui, snap, colors, q, size, widths))
                        .inner
                })
                .inner;
            if clicked {
                *selected = if is_selected { None } else { Some(q.form_id) };
            }
        }
    });
    changed
}

/// A fixed-width table cell, so long descriptions wrap instead of widening the table.
fn cell<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(width);
            add(ui)
        },
    )
    .inner
}

/// One quest. Returns true if it was clicked.
fn row(
    ui: &mut egui::Ui,
    snap: &Snapshot,
    colors: &QuestColors,
    q: &Quest,
    size: f32,
    [type_w, desc_w, done_w]: [f32; 3],
) -> bool {
    let mut clicked = false;
    let small = size * 0.7;
    cell(ui, type_w, |ui| {
        clicked |= ui
            .add(
                egui::Label::new(RichText::new(kind_label(q.kind)).size(small).color(DIM))
                    .sense(egui::Sense::click()),
            )
            .clicked();
    });

    cell(ui, desc_w, |ui| {
        ui.horizontal(|ui| {
            // A painted dot in the quest's marker colour (the default fonts have no circle glyph).
            // The space is kept for quests without markers so the names line up.
            let (r, dot) =
                ui.allocate_exact_size(egui::Vec2::splat(size * 0.6), egui::Sense::click());
            if let Some(c) = colors.quest(q.form_id) {
                ui.painter().circle_filled(r.center(), size * 0.25, c);
            }
            clicked |= dot.clicked();
            let (title, color) = match status(q) {
                QuestStatus::Tracked => (format!("\u{BB} {}", title(q)), GOLD),
                QuestStatus::Current => (title(q), Color32::LIGHT_GRAY),
                QuestStatus::Completed | QuestStatus::Unknown => (title(q), DIM),
            };
            let text = RichText::new(title).size(size * 0.85).color(color);
            clicked |= ui
                .add(egui::Label::new(text).wrap().sense(egui::Sense::click()))
                .on_hover_text("Show on map")
                .clicked();
        });
        let finished = status(q) == QuestStatus::Completed;
        if q.objectives
            .iter()
            .skip(usize::from(q.name.is_empty()))
            .any(|o| finished || o.state == ObjectiveState::Active)
        {
            ui.indent(("objectives", q.form_id), |ui| {
                clicked |= objectives(ui, snap, colors, q, size);
            });
        }
    });

    cell(ui, done_w, |ui| {
        let (text, color) = progress(q);
        ui.label(RichText::new(text).size(small).color(color));
    });
    clicked
}

/// The quest's status; older plugins send none, so fall back to the tracked flag.
fn status(q: &Quest) -> QuestStatus {
    match q.status {
        QuestStatus::Current if q.tracked => QuestStatus::Tracked,
        s => s,
    }
}

/// Quests without a name are the journal's "Miscellaneous" objectives; their first objective names them.
fn title(q: &Quest) -> String {
    if !q.name.is_empty() {
        return tidy_text(&q.name);
    }
    q.objectives
        .first()
        .map(|o| tidy_text(&o.text))
        .unwrap_or_else(|| q.editor_id.clone())
}

fn kind_label(kind: QuestKind) -> &'static str {
    match kind {
        QuestKind::Main => "Main",
        QuestKind::Faction => "Faction",
        QuestKind::Daedric => "Daedric",
        QuestKind::Side => "Side",
        QuestKind::Misc => "Misc",
        QuestKind::Dlc => "DLC",
    }
}

/// The Complete column: a tick for finished quests, objectives done so far for running ones.
fn progress(q: &Quest) -> (String, Color32) {
    let done = q
        .objectives
        .iter()
        .filter(|o| o.state == ObjectiveState::Completed)
        .count();
    let failed = q
        .objectives
        .iter()
        .any(|o| o.state == ObjectiveState::Failed);
    match status(q) {
        QuestStatus::Completed if failed && done == 0 => ("Failed".into(), RED),
        QuestStatus::Completed => ("Done".into(), GOLD),
        QuestStatus::Unknown => ("\u{2014}".into(), DIM),
        _ if q.objectives.is_empty() => ("\u{2014}".into(), DIM),
        _ => (
            format!("{done} / {}", q.objectives.len()),
            Color32::LIGHT_GRAY,
        ),
    }
}

/// Objective lines under the quest name: open ones for running quests, all of them for finished
/// ones. An open objective with a map marker gets its bullet in the quest's marker colour.
/// Returns true if an objective was clicked (which selects its quest).
fn objectives(
    ui: &mut egui::Ui,
    snap: &Snapshot,
    colors: &QuestColors,
    q: &Quest,
    size: f32,
) -> bool {
    let finished = status(q) == QuestStatus::Completed;
    // Unnamed quests already show their first objective as the title.
    let skip = usize::from(q.name.is_empty());
    let mut clicked = false;
    for o in q.objectives.iter().skip(skip) {
        let (mark, color) = match o.state {
            ObjectiveState::Active => ("\u{2022}", Color32::WHITE),
            // Plain glyphs: decorative fonts like Lucida Calligraphy lack check marks.
            ObjectiveState::Completed if finished => ("\u{2022}", DIM),
            ObjectiveState::Failed if finished => ("\u{D7}", RED),
            _ => continue,
        };
        let has_marker = snap
            .markers
            .iter()
            .any(|m| m.quest == Some(q.form_id) && m.name == o.text);
        let bullet_color = match colors.quest(q.form_id) {
            Some(c) if has_marker && o.state == ObjectiveState::Active => c,
            _ => color,
        };
        let font = size * 0.7;
        // Bullet and text side by side, so wrapped lines stay indented under the text.
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = size * 0.25;
            ui.label(RichText::new(mark).size(font).color(bullet_color));
            clicked |= ui
                .add(
                    egui::Label::new(strike(
                        RichText::new(tidy_text(&o.text)).size(font).color(color),
                        o.state == ObjectiveState::Failed,
                    ))
                    .wrap()
                    .sense(egui::Sense::click()),
                )
                .clicked();
        });
    }
    clicked
}

fn strike(text: RichText, on: bool) -> RichText {
    if on {
        text.strikethrough()
    } else {
        text
    }
}

/// Cleans journal text the plugin couldn't resolve: drops `<Global=…>`, `<Alias=…>` and other
/// tokens, stray `M_Global…` names, and the brackets or slashes they leave behind.
/// "Gather firewood (<Global=M_GlobalA>/<Global=B>)" becomes "Gather firewood".
pub fn tidy_text(text: &str) -> String {
    // Remove <...> tokens (an unclosed one runs to the next space or bracket).
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            while let Some(&n) = chars.peek() {
                if n == '>' {
                    chars.next();
                    break;
                }
                if n == ')' || n.is_whitespace() {
                    break;
                }
                chars.next();
            }
        } else {
            out.push(c);
        }
    }
    // Drop leftover variable names.
    let words: Vec<&str> = out
        .split_whitespace()
        .filter(|w| !w.contains("Global") && !w.starts_with("M_"))
        .collect();
    let mut out = words.join(" ");
    // Brackets left with only slashes, colons or spaces in them.
    let junk = |s: &str| s.chars().all(|c| matches!(c, '/' | ':' | ' ' | '='));
    let mut from = 0;
    while let Some(open) = out[from..].find('(').map(|i| from + i) {
        match out[open..].find(')') {
            Some(len) if junk(&out[open + 1..open + len]) => {
                out.replace_range(open..=open + len, "")
            }
            Some(len) => from = open + len,
            // Unclosed: drop it if nothing useful follows.
            None if junk(&out[open + 1..]) => out.truncate(open),
            None => break,
        }
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    out.replace(" .", ".").replace(" ,", ",").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::tidy_text;

    #[test]
    fn tidies_unresolved_tokens() {
        assert_eq!(
            tidy_text("Gather firewood (<Global=M_GlobalA>/<Global=B>)"),
            "Gather firewood"
        );
        assert_eq!(tidy_text("Kill <Alias=Target> ."), "Kill.");
        assert_eq!(tidy_text("Find (<Global=M_GlobalX"), "Find");
        assert_eq!(
            tidy_text("Collect Nirnroot (12/30)"),
            "Collect Nirnroot (12/30)"
        );
        assert_eq!(tidy_text("Plain text"), "Plain text");
    }
}
