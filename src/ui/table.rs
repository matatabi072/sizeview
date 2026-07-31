use crate::config::SortKey;
use crate::fs_model::Entry;
use chrono::{DateTime, Local};
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use humansize::{format_size, BINARY};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
pub struct SortState {
    pub key: SortKey,
    pub asc: bool,
}

pub enum TableAction {
    None,
    Select(usize),
    Open(PathBuf),
    ContextOpenExplorer(PathBuf),
    ContextCopyPath(String),
    ContextRecalc(PathBuf),
}

pub fn draw(
    ui: &mut egui::Ui,
    entries: &[Entry],
    sort: &mut SortState,
    selected: Option<usize>,
    icons: &mut crate::icons::IconCache,
) -> TableAction {
    let mut action = TableAction::None;
    let row_h = egui::TextStyle::Body.resolve(ui.style()).size + 8.0;
    let icon_size = row_h - 4.0;

    let mut table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(340.0).at_least(140.0).clip(true))
        .column(Column::initial(110.0).at_least(70.0).clip(true))
        .column(Column::initial(160.0).at_least(100.0).clip(true))
        .column(Column::remainder().at_least(100.0).clip(true))
        .min_scrolled_height(0.0)
        .auto_shrink([false, false]);

    if let Some(sel) = selected {
        table = table.scroll_to_row(sel, None);
    }

    table
        .header(24.0, |mut header| {
            header_cell(&mut header, "名前", SortKey::Name, sort);
            header_cell(&mut header, "サイズ", SortKey::Size, sort);
            header_cell(&mut header, "更新日時", SortKey::Modified, sort);
            header_cell(&mut header, "種類", SortKey::Kind, sort);
        })
        .body(|mut body| {
            body.ui_mut().style_mut().interaction.selectable_labels = false;
            body.rows(row_h, entries.len(), |mut row| {
                let idx = row.index();
                let e = &entries[idx];
                if Some(idx) == selected {
                    row.set_selected(true);
                }

                row.col(|ui| {
                    if let Some(tex) = icons.for_entry(e) {
                        ui.add(egui::Image::new(&tex).fit_to_exact_size(egui::vec2(icon_size, icon_size)));
                    } else {
                        let ch = if e.is_dir { "📁" } else { "📄" };
                        ui.label(ch);
                    }
                    ui.label(&e.name);
                    if let Some(sfx) = &e.info_suffix {
                        ui.label(muted(ui, sfx));
                    }
                });
                row.col(|ui| {
                    let text = match e.size {
                        Some(n) => format_size(n, BINARY),
                        None => "…".to_string(),
                    };
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.monospace(text);
                        },
                    );
                });
                row.col(|ui| {
                    ui.monospace(format_mtime(e.mtime_unix));
                });
                row.col(|ui| {
                    ui.label(&e.kind_label);
                });

                let resp = row.response();
                if resp.clicked() {
                    action = TableAction::Select(idx);
                }
                if resp.double_clicked() && e.is_dir {
                    action = TableAction::Open(e.path.clone());
                }
                resp.context_menu(|ui| {
                    if !matches!(action, TableAction::None) {
                        // 別のアクションが既に決まっていれば触らない (安全側)
                    }
                    if ui.button("エクスプローラーで開く").clicked() {
                        action = TableAction::ContextOpenExplorer(e.path.clone());
                        ui.close_menu();
                    }
                    if ui.button("パスをコピー").clicked() {
                        action = TableAction::ContextCopyPath(e.path.display().to_string());
                        ui.close_menu();
                    }
                    if e.is_dir {
                        ui.separator();
                        if ui.button("サイズを再計算").clicked() {
                            action = TableAction::ContextRecalc(e.path.clone());
                            ui.close_menu();
                        }
                    }
                });
            });
        });

    action
}

fn muted(ui: &egui::Ui, s: &str) -> egui::RichText {
    // pitfall #11 対策: .weak() は暗背景で薄すぎるので text_color と背景を 80:20 で混ぜる
    let base = ui.visuals().text_color();
    let bg = ui.visuals().extreme_bg_color;
    let blend = |t: u8, b: u8| ((t as u16 * 82 + b as u16 * 18) / 100) as u8;
    let color = egui::Color32::from_rgb(
        blend(base.r(), bg.r()),
        blend(base.g(), bg.g()),
        blend(base.b(), bg.b()),
    );
    egui::RichText::new(s).color(color)
}

fn format_mtime(mtime_unix: Option<i64>) -> String {
    match mtime_unix {
        Some(t) if t > 0 => DateTime::from_timestamp(t, 0)
            .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "-".into()),
        _ => "-".into(),
    }
}

fn header_cell(
    header: &mut egui_extras::TableRow<'_, '_>,
    label: &str,
    key: SortKey,
    sort: &mut SortState,
) {
    header.col(|ui| {
        let mark = if sort.key == key {
            if sort.asc { " ▲" } else { " ▼" }
        } else {
            ""
        };
        let text = egui::RichText::new(format!("{}{}", label, mark)).strong();
        if ui.add(egui::Button::new(text).frame(false)).clicked() {
            if sort.key == key {
                sort.asc = !sort.asc;
            } else {
                sort.key = key;
                sort.asc = matches!(key, SortKey::Name | SortKey::Kind);
            }
        }
    });
}

/// 表示用に entries をソート済みで返す。フォルダは常に先頭。
pub fn sort_entries(entries: &mut [Entry], sort: SortState) {
    entries.sort_by(|a, b| compare(a, b, sort));
}

fn compare(a: &Entry, b: &Entry, sort: SortState) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a.is_dir, b.is_dir) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }
    let ord = match sort.key {
        SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        SortKey::Size => a.size.unwrap_or(0).cmp(&b.size.unwrap_or(0)),
        SortKey::Modified => a.mtime_unix.unwrap_or(0).cmp(&b.mtime_unix.unwrap_or(0)),
        SortKey::Kind => a
            .kind_label
            .to_lowercase()
            .cmp(&b.kind_label.to_lowercase()),
    };
    if sort.asc {
        ord
    } else {
        ord.reverse()
    }
}
