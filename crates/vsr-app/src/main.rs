//! vscode-rust: the native workbench window.
//!
//! Layout mirrors the VS Code workbench parts: title bar, activity bar,
//! side bar (explorer), editor group (tabs + editor), status bar.
//!
//! upstream: src/vs/workbench/browser/layout.ts @ 50e1350fc78d

mod icons;
mod model;
mod theme;

use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Frame, Layout, Margin, RichText, Sense, Stroke,
    TextStyle, Vec2,
};
use model::{DirEntry, Editors, list_dir};
use theme::{Theme, metrics as m};

fn main() -> eframe::Result {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    let root = root.canonicalize().unwrap_or(root);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("vscode-rust")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 400.0])
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false),
        ..Default::default()
    };
    eframe::run_native(
        "vscode-rust",
        options,
        Box::new(move |cc| Ok(Box::new(Workbench::new(cc, root)))),
    )
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum ActivityView {
    Explorer,
    Search,
    Scm,
    Extensions,
}

struct Workbench {
    theme: Theme,
    root: PathBuf,
    view: ActivityView,
    sidebar_visible: bool,
    expanded: std::collections::HashSet<PathBuf>,
    editors: Editors,
    branch: Option<String>,
    error: Option<String>,
}

impl Workbench {
    fn new(cc: &eframe::CreationContext<'_>, root: PathBuf) -> Self {
        let theme = Theme::dark_modern();
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = theme.chrome_bg;
        visuals.window_fill = theme.editor_bg;
        visuals.extreme_bg_color = theme.editor_bg;
        visuals.override_text_color = Some(theme.sidebar_fg);
        visuals.selection.bg_fill = theme.accent.gamma_multiply(0.5);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, theme.border);
        cc.egui_ctx.set_visuals(visuals);
        icons::install(&cc.egui_ctx);

        let mut wb = Self {
            theme,
            root: root.clone(),
            view: ActivityView::Explorer,
            sidebar_visible: true,
            expanded: Default::default(),
            editors: Editors::default(),
            branch: git_branch(&root),
            error: None,
        };
        // Open a README if there is one, so the first launch shows real text.
        for name in ["README.md", "Cargo.toml"] {
            let p = root.join(name);
            if p.is_file() {
                wb.open(&p);
                break;
            }
        }
        wb
    }

    fn open(&mut self, path: &Path) {
        if let Err(e) = self.editors.open_file(path) {
            self.error = Some(format!("{}: {e}", path.display()));
        }
    }

    // ---- parts --------------------------------------------------------------

    fn title_bar(&self, ui: &mut egui::Ui) {
        let t = &self.theme;
        egui::Panel::top("titlebar")
            .exact_size(m::TITLE_BAR_HEIGHT)
            .resizable(false)
            .frame(Frame::new().fill(t.chrome_bg).stroke(Stroke::new(1.0, t.border)))
            .show(ui, |ui| {
                let title = match self.editors.active_editor() {
                    Some(e) => format!("{} — {}", e.title, folder_name(&self.root)),
                    None => folder_name(&self.root),
                };
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new(title).size(13.0).color(t.sidebar_fg));
                });
            });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        let t = &self.theme;
        egui::Panel::bottom("statusbar")
            .exact_size(m::STATUS_BAR_HEIGHT)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(t.chrome_bg)
                    .stroke(Stroke::new(1.0, t.border))
                    .inner_margin(Margin::symmetric(10, 0)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let small = |s: &str| RichText::new(s).size(12.0).color(t.sidebar_fg);
                    if let Some(branch) = &self.branch {
                        ui.label(small(&format!("{} {branch}", icons::GIT_BRANCH)));
                    }
                    ui.add_space(8.0);
                    // Problems counter (no diagnostics source yet, so always zero).
                    ui.label(small(&format!("{} 0  {} 0", icons::ERROR, icons::WARNING)));
                    if let Some(err) = &self.error {
                        ui.label(RichText::new(err).size(12.0).color(Color32::from_rgb(0xF1, 0x4C, 0x4C)));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(small(&icons::BELL.to_string())).on_hover_text("No Notifications");
                        ui.add_space(12.0);
                        if let Some(e) = self.editors.active_editor() {
                            ui.label(small(language_of(&e.path)));
                            ui.add_space(12.0);
                            ui.label(small(e.eol_label()));
                            ui.add_space(12.0);
                            ui.label(small("UTF-8"));
                            ui.add_space(12.0);
                            ui.label(small(&format!("{} lines", e.buffer.line_count())));
                        }
                    });
                });
            });
    }

    fn activity_bar(&mut self, ui: &mut egui::Ui) {
        let t = self.theme.clone();
        egui::Panel::left("activitybar")
            .exact_size(m::ACTIVITY_BAR_WIDTH)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(t.chrome_bg).stroke(Stroke::new(1.0, t.border)))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for (view, icon, tip) in [
                    (ActivityView::Explorer, icons::FILES, "Explorer (⇧⌘E)"),
                    (ActivityView::Search, icons::SEARCH, "Search (⇧⌘F)"),
                    (ActivityView::Scm, icons::SOURCE_CONTROL, "Source Control (⌃⇧G)"),
                    (ActivityView::Extensions, icons::EXTENSIONS, "Extensions (⇧⌘X)"),
                ] {
                    let active = self.sidebar_visible && self.view == view;
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::splat(m::ACTIVITY_BAR_WIDTH), Sense::click());
                    let fg = if active || resp.hovered() { t.activity_fg } else { t.activity_inactive_fg };
                    if active {
                        let bar = egui::Rect::from_min_size(rect.min, Vec2::new(2.0, rect.height()));
                        ui.painter().rect_filled(bar, CornerRadius::ZERO, t.accent);
                    }
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, icons::font(24.0), fg);
                    if resp.on_hover_text(tip).clicked() {
                        // Clicking the active view toggles the side bar, like VS Code.
                        if active {
                            self.sidebar_visible = false;
                        } else {
                            self.view = view;
                            self.sidebar_visible = true;
                        }
                    }
                }
                // Global activities pinned to the bottom, like VS Code.
                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    for (icon, tip) in [(icons::SETTINGS_GEAR, "Manage"), (icons::ACCOUNT, "Accounts")] {
                        let (rect, resp) =
                            ui.allocate_exact_size(Vec2::splat(m::ACTIVITY_BAR_WIDTH), Sense::click());
                        let fg = if resp.hovered() { t.activity_fg } else { t.activity_inactive_fg };
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, icons::font(24.0), fg);
                        resp.on_hover_text(tip);
                    }
                });
            });
    }

    fn side_bar(&mut self, ui: &mut egui::Ui) {
        if !self.sidebar_visible {
            return;
        }
        let t = self.theme.clone();
        egui::Panel::left("sidebar")
            .default_size(m::SIDEBAR_WIDTH)
            .size_range(170.0..=600.0)
            .resizable(true)
            .frame(Frame::new().fill(t.chrome_bg).stroke(Stroke::new(1.0, t.border)))
            .show(ui, |ui| {
                let heading = match self.view {
                    ActivityView::Explorer => "EXPLORER",
                    ActivityView::Search => "SEARCH",
                    ActivityView::Scm => "SOURCE CONTROL",
                    ActivityView::Extensions => "EXTENSIONS",
                };
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    ui.label(RichText::new(heading).size(11.0).color(t.sidebar_fg));
                });
                ui.add_space(8.0);
                match self.view {
                    ActivityView::Explorer => {
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            ui.label(RichText::new(format!("{} {}", icons::CHEVRON_DOWN, folder_name(&self.root).to_uppercase())).size(11.0).strong());
                        });
                        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            let root = self.root.clone();
                            self.tree(ui, &root, 0);
                        });
                    }
                    _ => {
                        ui.horizontal(|ui| {
                            ui.add_space(20.0);
                            ui.label(RichText::new("Not implemented yet.").size(12.0).color(t.tab_inactive_fg));
                        });
                    }
                }
            });
    }

    fn tree(&mut self, ui: &mut egui::Ui, dir: &Path, depth: usize) {
        let entries: Vec<DirEntry> = list_dir(dir).unwrap_or_default();
        let active = self.editors.active_editor().map(|e| e.path.clone());
        for entry in entries {
            let t = &self.theme;
            let (rect, resp) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::click());
            let is_active = active.as_deref() == Some(entry.path.as_path());
            if is_active {
                ui.painter().rect_filled(rect, CornerRadius::ZERO, t.list_active_bg);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, CornerRadius::ZERO, t.list_hover_bg);
            }
            let open = self.expanded.contains(&entry.path);
            let twistie = match (entry.is_dir, open) {
                (true, true) => icons::CHEVRON_DOWN.to_string(),
                (true, false) => icons::CHEVRON_RIGHT.to_string(),
                _ => String::new(),
            };
            let x = rect.left() + 8.0 + depth as f32 * 8.0;
            let font = FontId::proportional(13.0);
            ui.painter().text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, twistie, icons::font(16.0), t.sidebar_fg);
            ui.painter().text(egui::pos2(x + 16.0, rect.center().y), egui::Align2::LEFT_CENTER, &entry.name, font, t.sidebar_fg);
            if resp.clicked() {
                if entry.is_dir {
                    if !self.expanded.remove(&entry.path) {
                        self.expanded.insert(entry.path.clone());
                    }
                } else {
                    self.open(&entry.path);
                }
            }
            if entry.is_dir && self.expanded.contains(&entry.path) {
                self.tree(ui, &entry.path, depth + 1);
            }
        }
    }

    fn editor_group(&mut self, ui: &mut egui::Ui) {
        let t = self.theme.clone();
        egui::CentralPanel::no_frame().show(ui, |ui| {
            // Tab strip
            egui::Panel::top("tabs")
                .exact_size(m::TAB_HEIGHT)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().fill(t.chrome_bg))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    ui.horizontal(|ui| {
                        let mut to_close = None;
                        let mut to_focus = None;
                        for (i, e) in self.editors.open.iter().enumerate() {
                            let active = self.editors.active == Some(i);
                            let font = FontId::proportional(13.0);
                            let text_w = ui.fonts_mut(|f| f.layout_no_wrap(e.title.clone(), font.clone(), Color32::WHITE).size().x);
                            let w = text_w + 56.0;
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, m::TAB_HEIGHT), Sense::click());
                            let p = ui.painter();
                            p.rect_filled(rect, CornerRadius::ZERO, if active { t.tab_active_bg } else { t.chrome_bg });
                            p.vline(rect.right(), rect.y_range(), Stroke::new(1.0, t.border));
                            if active {
                                p.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, t.accent));
                            } else {
                                p.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, t.border));
                            }
                            let fg = if active { t.tab_active_fg } else { t.tab_inactive_fg };
                            p.text(egui::pos2(rect.left() + 12.0, rect.center().y), egui::Align2::LEFT_CENTER, &e.title, font, fg);
                            let close = egui::Rect::from_center_size(egui::pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(16.0));
                            let hovering_close = resp.hover_pos().is_some_and(|p| close.contains(p));
                            if active || resp.hovered() {
                                p.text(close.center(), egui::Align2::CENTER_CENTER, icons::CLOSE, icons::font(16.0), fg);
                            }
                            if resp.clicked() {
                                if hovering_close { to_close = Some(i) } else { to_focus = Some(i) }
                            }
                            if resp.middle_clicked() {
                                to_close = Some(i);
                            }
                        }
                        // Fill the rest of the strip's bottom border.
                        let rest = ui.available_rect_before_wrap();
                        ui.painter().hline(rest.x_range(), rest.bottom() - 0.5, Stroke::new(1.0, t.border));
                        if let Some(i) = to_focus {
                            self.editors.active = Some(i);
                        }
                        if let Some(i) = to_close {
                            self.editors.close(i);
                        }
                    });
                });

            // Editor surface
            egui::CentralPanel::no_frame()
                .frame(Frame::new().fill(t.editor_bg))
                .show(ui, |ui| match self.editors.active_editor() {
                    Some(e) => text_view(ui, &t, e),
                    None => {
                        ui.centered_and_justified(|ui| {
                            ui.label(RichText::new("vscode-rust\n\nOpen a file from the Explorer").size(16.0).color(t.tab_inactive_fg));
                        });
                    }
                });
        });
    }
}

/// Read-only, virtualised text view: only visible lines are laid out.
fn text_view(ui: &mut egui::Ui, t: &Theme, e: &model::OpenEditor) {
    let font = FontId::monospace(m::EDITOR_FONT_SIZE);
    let lines = e.buffer.line_count();
    let gutter_chars = lines.to_string().len().max(3);
    let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
    let gutter_w = char_w * gutter_chars as f32 + 34.0;
    egui::ScrollArea::both()
        .id_salt(&e.path)
        .auto_shrink(false)
        .show_rows(ui, m::EDITOR_LINE_HEIGHT, lines, |ui, range| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for line in range.start + 1..=range.end {
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width().max(gutter_w + 200.0), m::EDITOR_LINE_HEIGHT),
                    Sense::hover(),
                );
                let y = rect.center().y;
                let p = ui.painter();
                p.text(
                    egui::pos2(rect.left() + gutter_w - 26.0, y),
                    egui::Align2::RIGHT_CENTER,
                    line.to_string(),
                    font.clone(),
                    if line == 1 { t.line_number_active_fg } else { t.line_number_fg },
                );
                p.text(
                    egui::pos2(rect.left() + gutter_w, y),
                    egui::Align2::LEFT_CENTER,
                    e.buffer.line_content(line).replace('\t', "    "),
                    font.clone(),
                    t.editor_fg,
                );
            }
        });
}

impl eframe::App for Workbench {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.style_mut().override_text_style = Some(TextStyle::Body);
        self.title_bar(ui);
        self.status_bar(ui);
        self.activity_bar(ui);
        self.side_bar(ui);
        self.editor_group(ui);
    }
}

/// Current branch of the folder's git repo, if any (placeholder until an SCM service exists).
fn git_branch(root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    let name = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !name.is_empty()).then_some(name)
}

fn folder_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

fn language_of(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "rs" => "Rust",
        "md" => "Markdown",
        "toml" => "TOML",
        "ts" => "TypeScript",
        "js" => "JavaScript",
        "json" => "JSON",
        "py" => "Python",
        _ => "Plain Text",
    }
}
