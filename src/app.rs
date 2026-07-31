use crate::cache::SizeCache;
use crate::config::{AppConfig, Theme};
use crate::fs_model::{self, Entry, Location};
use crate::icons::IconCache;
use crate::size_worker::SizeWorker;
use crate::ui::{
    table::{self, SortState, TableAction},
    toolbar::{self, ToolbarAction, ToolbarState},
};
use eframe::egui;
use std::path::PathBuf;

pub struct App {
    config: AppConfig,

    location: Location,
    back_stack: Vec<Location>,
    forward_stack: Vec<Location>,

    all_entries: Vec<Entry>,
    selected_path: Option<PathBuf>,

    worker: SizeWorker,
    cache: SizeCache,
    scanning: usize,

    icons: IconCache,
    /// タイトルバーへのテーマ適用が済んだか。窓ハンドル取得失敗時 (最初の 1〜数フレーム)
    /// に false のまま、次フレームで再試行する。
    theme_applied: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, config: AppConfig) -> Self {
        crate::fonts::setup(&cc.egui_ctx);
        let worker = SizeWorker::new(cc.egui_ctx.clone());
        let cache = SizeCache::load_default();
        let icons = IconCache::new(cc.egui_ctx.clone());
        let mut app = Self {
            config,
            location: Location::Drives,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            all_entries: Vec::new(),
            selected_path: None,
            worker,
            cache,
            scanning: 0,
            icons,
            theme_applied: false,
        };
        app.reload();
        app
    }

    fn reload(&mut self) {
        self.selected_path = None;
        let gen = self.worker.bump_generation();
        self.all_entries = match &self.location {
            Location::Drives => fs_model::enumerate_drives(),
            Location::Folder(p) => fs_model::read_folder(p),
        };

        let mut new_reqs = 0usize;
        for e in &mut self.all_entries {
            if !e.is_dir {
                continue;
            }
            let mtime = e.mtime_unix.unwrap_or(0);
            if let Some((size, _err)) = self.cache.get(&e.path, mtime) {
                e.size = Some(size);
            } else {
                self.worker.request(e.path.clone(), mtime, gen);
                new_reqs += 1;
            }
        }
        self.scanning = new_reqs;
    }

    fn navigate(&mut self, loc: Location) {
        if self.location == loc {
            return;
        }
        let old = std::mem::replace(&mut self.location, loc);
        self.back_stack.push(old);
        self.forward_stack.clear();
        self.reload();
    }

    fn nav_back(&mut self) {
        if let Some(prev) = self.back_stack.pop() {
            let cur = std::mem::replace(&mut self.location, prev);
            self.forward_stack.push(cur);
            self.reload();
        }
    }

    fn nav_forward(&mut self) {
        if let Some(next) = self.forward_stack.pop() {
            let cur = std::mem::replace(&mut self.location, next);
            self.back_stack.push(cur);
            self.reload();
        }
    }

    fn drain_results(&mut self) {
        let gen = self.worker.current_generation();
        let results = self.worker.try_drain();
        for r in results {
            self.cache.insert(&r.path, r.size, r.mtime_unix, r.err);
            if r.generation < gen {
                continue;
            }
            for e in &mut self.all_entries {
                if e.is_dir && e.path == r.path {
                    e.size = Some(r.size);
                    if self.scanning > 0 {
                        self.scanning -= 1;
                    }
                }
            }
        }
    }

    fn sorted_visible(&self) -> Vec<Entry> {
        let mut list: Vec<Entry> = if self.config.show_files {
            self.all_entries.clone()
        } else {
            self.all_entries
                .iter()
                .filter(|e| e.is_dir)
                .cloned()
                .collect()
        };
        table::sort_entries(
            &mut list,
            SortState {
                key: self.config.sort_key,
                asc: self.config.sort_asc,
            },
        );
        list
    }

    fn set_theme(&mut self, ctx: &egui::Context, t: Theme) {
        self.config.theme = t;
        // メニュー操作時は SizeView 窓がフォアグラウンドなので基本成功する。
        // 万一失敗 (窓が pop-up 直後で FindWindowW ミス等) しても次フレームで再試行。
        self.theme_applied = crate::theme::apply(ctx, t);
    }

    fn open_in_explorer(path: &std::path::Path) {
        let is_dir = path.is_dir();
        if is_dir {
            let _ = std::process::Command::new("explorer.exe")
                .arg(path)
                .spawn();
        } else {
            // ファイル: 選択状態で親フォルダを開く
            let arg = format!("/select,{}", path.display());
            let _ = std::process::Command::new("explorer.exe")
                .arg(arg)
                .spawn();
        }
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context, sorted: &[Entry]) -> Option<Location> {
        let (down, up, enter, backspace, f5, home, end, alt_left, alt_right) =
            ctx.input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::Backspace),
                    i.key_pressed(egui::Key::F5),
                    i.key_pressed(egui::Key::Home),
                    i.key_pressed(egui::Key::End),
                    i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft),
                    i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight),
                )
            });

        let cur = self
            .selected_path
            .as_ref()
            .and_then(|p| sorted.iter().position(|e| &e.path == p));

        if down && !sorted.is_empty() {
            let n = match cur {
                Some(i) => (i + 1).min(sorted.len() - 1),
                None => 0,
            };
            self.selected_path = Some(sorted[n].path.clone());
        } else if up && !sorted.is_empty() {
            let n = match cur {
                Some(i) => i.saturating_sub(1),
                None => 0,
            };
            self.selected_path = Some(sorted[n].path.clone());
        } else if home && !sorted.is_empty() {
            self.selected_path = Some(sorted[0].path.clone());
        } else if end && !sorted.is_empty() {
            self.selected_path = Some(sorted[sorted.len() - 1].path.clone());
        }

        if enter {
            if let Some(i) = cur {
                let e = &sorted[i];
                if e.is_dir {
                    return Some(Location::Folder(e.path.clone()));
                }
            }
        }
        if backspace {
            if let Some(p) = self.location.parent() {
                return Some(p);
            }
        }
        if f5 {
            for e in &self.all_entries.clone() {
                if e.is_dir {
                    self.cache.invalidate(&e.path);
                }
            }
            self.reload();
        }
        if alt_left {
            self.nav_back();
        }
        if alt_right {
            self.nav_forward();
        }
        None
    }

    fn save_window_size(&mut self, ctx: &egui::Context) {
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            let w = rect.width();
            let h = rect.height();
            if w > 100.0 && h > 100.0 {
                self.config.window_w = w;
                self.config.window_h = h;
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.theme_applied {
            self.theme_applied = crate::theme::apply(ctx, self.config.theme);
            if !self.theme_applied {
                // 窓ハンドルが取れるまで積極的に再試行
                ctx.request_repaint();
            }
        }

        self.drain_results();
        self.save_window_size(ctx);

        // トップバー
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(4.0);
            let state = ToolbarState {
                location: &self.location,
                show_files: self.config.show_files,
                scanning: self.scanning,
                can_back: !self.back_stack.is_empty(),
                can_forward: !self.forward_stack.is_empty(),
                theme: self.config.theme,
            };
            let action = toolbar::draw(ui, state);
            ui.add_space(4.0);
            match action {
                ToolbarAction::None => {}
                ToolbarAction::Back => self.nav_back(),
                ToolbarAction::Forward => self.nav_forward(),
                ToolbarAction::Up => {
                    if let Some(p) = self.location.parent() {
                        self.navigate(p);
                    }
                }
                ToolbarAction::GoDrives => self.navigate(Location::Drives),
                ToolbarAction::Go(p) => self.navigate(Location::Folder(p)),
                ToolbarAction::ToggleFiles(v) => {
                    self.config.show_files = v;
                    self.selected_path = None;
                }
                ToolbarAction::Refresh => {
                    for e in &self.all_entries.clone() {
                        if e.is_dir {
                            self.cache.invalidate(&e.path);
                        }
                    }
                    self.reload();
                }
                ToolbarAction::SetTheme(t) => {
                    self.set_theme(ctx, t);
                }
            }
        });

        // ステータスバー
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let (fld, fil) =
                    self.all_entries.iter().fold((0usize, 0usize), |(a, b), e| {
                        if e.is_dir {
                            (a + 1, b)
                        } else {
                            (a, b + 1)
                        }
                    });
                ui.label(format!("{} フォルダ / {} ファイル", fld, fil));
                if let Some(p) = &self.selected_path {
                    if let Some(e) = self.all_entries.iter().find(|x| &x.path == p) {
                        ui.separator();
                        ui.label(&e.name);
                    }
                }
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if crate::build_info::IS_DEBUG {
                            ui.colored_label(
                                egui::Color32::from_rgb(230, 190, 60),
                                "DEBUG",
                            );
                            ui.separator();
                        }
                        ui.label(self.location.display());
                        ui.separator();
                        ui.label(format!("cache: {}", self.cache.len()));
                    },
                );
            });
            ui.add_space(2.0);
        });

        // 中央: テーブル
        egui::CentralPanel::default().show(ctx, |ui| {
            let sorted = self.sorted_visible();
            let sel_idx = self
                .selected_path
                .as_ref()
                .and_then(|p| sorted.iter().position(|e| &e.path == p));
            let mut sort_state = SortState {
                key: self.config.sort_key,
                asc: self.config.sort_asc,
            };

            let action = table::draw(ui, &sorted, &mut sort_state, sel_idx, &mut self.icons);
            self.config.sort_key = sort_state.key;
            self.config.sort_asc = sort_state.asc;

            match action {
                TableAction::None => {}
                TableAction::Select(idx) => {
                    if let Some(e) = sorted.get(idx) {
                        self.selected_path = Some(e.path.clone());
                    }
                }
                TableAction::Open(p) => {
                    self.navigate(Location::Folder(p));
                }
                TableAction::ContextOpenExplorer(p) => Self::open_in_explorer(&p),
                TableAction::ContextCopyPath(s) => {
                    ctx.output_mut(|o| o.copied_text = s);
                }
                TableAction::ContextRecalc(p) => {
                    self.cache.invalidate(&p);
                    // このパス限定で再スキャン: 世代を進めて自分だけ再要求
                    let gen = self.worker.bump_generation();
                    // 現在の Entry の mtime を渡す
                    let mtime = self
                        .all_entries
                        .iter()
                        .find(|e| e.path == p)
                        .and_then(|e| e.mtime_unix)
                        .unwrap_or(0);
                    // 表示を「計算中」に戻す
                    for e in &mut self.all_entries {
                        if e.path == p {
                            e.size = None;
                        }
                    }
                    self.worker.request(p, mtime, gen);
                    self.scanning += 1;
                    // 同じ画面のほかの (キャッシュ済) フォルダ表示は変えない → 再要求もしない
                }
            }

            // キーボード: sorted を渡してナビ用途に使う
            if let Some(nav) = self.handle_keyboard(ctx, &sorted) {
                self.navigate(nav);
            }
        });

        if self.scanning > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        // eframe が定期 (30 秒毎) と on_exit 前に呼ぶ
        self.config.save();
        self.cache.save_if_dirty();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.config.save();
        self.cache.save_if_dirty();
    }
}
