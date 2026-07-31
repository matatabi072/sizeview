use crate::config::Theme;
use crate::fs_model::Location;
use eframe::egui;
use std::path::PathBuf;

pub enum ToolbarAction {
    None,
    Back,
    Forward,
    Up,
    GoDrives,
    Go(PathBuf),
    ToggleFiles(bool),
    Refresh,
    SetTheme(Theme),
}

pub struct ToolbarState<'a> {
    pub location: &'a Location,
    pub show_files: bool,
    pub scanning: usize,
    pub can_back: bool,
    pub can_forward: bool,
    pub theme: Theme,
}

pub fn draw(ui: &mut egui::Ui, s: ToolbarState) -> ToolbarAction {
    let mut action = ToolbarAction::None;

    ui.horizontal(|ui| {
        if ui
            .add_enabled(s.can_back, egui::Button::new("← 戻る"))
            .on_hover_text("戻る (Alt+←)")
            .clicked()
        {
            action = ToolbarAction::Back;
        }
        if ui
            .add_enabled(s.can_forward, egui::Button::new("進む →"))
            .on_hover_text("進む (Alt+→)")
            .clicked()
        {
            action = ToolbarAction::Forward;
        }
        let can_up = s.location.parent().is_some();
        if ui
            .add_enabled(can_up, egui::Button::new("↑ 上へ"))
            .on_hover_text("上へ (Backspace)")
            .clicked()
        {
            action = ToolbarAction::Up;
        }
        if ui
            .button("🖥 PC")
            .on_hover_text("ドライブ一覧に戻る")
            .clicked()
        {
            action = ToolbarAction::GoDrives;
        }
        if ui.button("🔄").on_hover_text("再読み込み (F5)").clicked() {
            action = ToolbarAction::Refresh;
        }

        ui.separator();
        breadcrumb(ui, s.location, &mut action);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // テーマ切替 (メニュー式)
            let effective_dark = matches!(
                crate::theme::resolve(s.theme),
                crate::theme::Effective::Dark
            );
            let icon = if effective_dark { "🌙" } else { "☀" };
            ui.menu_button(icon, |ui| {
                if ui
                    .selectable_label(s.theme == Theme::System, "OS に従う")
                    .clicked()
                {
                    action = ToolbarAction::SetTheme(Theme::System);
                    ui.close_menu();
                }
                if ui
                    .selectable_label(s.theme == Theme::Dark, "ダーク")
                    .clicked()
                {
                    action = ToolbarAction::SetTheme(Theme::Dark);
                    ui.close_menu();
                }
                if ui
                    .selectable_label(s.theme == Theme::Light, "ライト")
                    .clicked()
                {
                    action = ToolbarAction::SetTheme(Theme::Light);
                    ui.close_menu();
                }
            })
            .response
            .on_hover_text("テーマ (現在: 有効=".to_string()
                + if effective_dark { "Dark" } else { "Light" }
                + ")");
            if s.scanning > 0 {
                ui.label(
                    egui::RichText::new(format!("計算中: {}", s.scanning))
                        .color(egui::Color32::from_rgb(200, 160, 60)),
                );
            }
            let mut sf = s.show_files;
            if ui.checkbox(&mut sf, "ファイルを表示").changed() {
                action = ToolbarAction::ToggleFiles(sf);
            }
        });
    });

    action
}

fn breadcrumb(ui: &mut egui::Ui, location: &Location, action: &mut ToolbarAction) {
    match location {
        Location::Drives => {
            ui.label(egui::RichText::new("PC").strong());
        }
        Location::Folder(p) => {
            if ui.link("PC").clicked() {
                *action = ToolbarAction::GoDrives;
            }
            ui.label(">");

            let mut accum = PathBuf::new();
            let comps: Vec<_> = p.components().collect();
            for (i, comp) in comps.iter().enumerate() {
                let seg = comp.as_os_str().to_string_lossy().into_owned();
                accum.push(comp);
                let is_last = i + 1 == comps.len();
                let label = seg.trim_end_matches(['\\', '/']).to_string();
                if is_last {
                    ui.label(egui::RichText::new(label).strong());
                } else {
                    if ui.link(label).clicked() {
                        *action = ToolbarAction::Go(accum.clone());
                    }
                    ui.label(">");
                }
            }
        }
    }
}
