//! The Welcome page: an editor tab with the Start actions and the recent files, shown when rscode
//! starts with nothing to restore and `workbench.startupEditor` is `welcomePage` (the default).
//!
//! upstream: src/vs/workbench/contrib/welcomeGettingStarted/browser/gettingStarted.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//! upstream: src/vs/workbench/contrib/welcomeGettingStarted/browser/startupPage.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//! upstream: src/vs/workbench/contrib/welcomeGettingStarted/browser/media/gettingStarted.css @ 50e1350fc78df67b11049b903d96b530f28d26a5
//!
//! The layout follows `gettingStarted.css` (heading 2.7em, subtitle 2em in the description colour,
//! section headings 1.5em, links in `textLink.foreground`, the recent list with the folder dimmed,
//! the "Show welcome page on startup" checkbox at the bottom) with the colours of the active
//! theme. What upstream has and this page leaves out, and why:
//!
//! - New File...: this page does not create a file. File: New Untitled Text File (Cmd+N) does (ADR-0058).
//! - Open Folder... / Clone Git Repository / Connect to...: no folder dialog, no git clone, no remotes.
//! - Walkthroughs, Help links, videos: they come from extensions and the web. The `workbench.welcomePage.*`
//!   settings only configure those, so none of them applies.
//! - Recent *folders*: they are remembered (`recents.json`) but cannot be opened without a folder picker.
//!
//! The page is one tab, like the Settings editor: it is not a file, so it lives next to the file
//! editors and is not part of `Editors`. It is not restored on the next run; whether it shows is
//! decided again at startup.

use std::path::{Path, PathBuf};

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::config::AppConfig;
use crate::config::settings::{StartupEditor, startup_editor};
use crate::theme::Rgba8;
use crate::ui::to_gpui;
use crate::{Workbench, icons};

/// How many recent files the page lists (upstream shows 5, then "More...").
pub const RECENT_LIMIT: usize = 5;

/// The command that opens the page (`Help: Welcome`).
pub const COMMAND_ID: &str = "workbench.action.openWalkthrough";

actions!(rscode, [ShowWelcome]);

/// `dir` shortened with `~` for the home folder, as the recent list shows it.
pub fn display_dir(dir: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| dir.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => dir.display().to_string(),
    }
}

impl Workbench {
    /// Whether the Welcome page is the tab being shown.
    pub(crate) fn welcome_showing(&self) -> bool {
        self.welcome_open && self.welcome_active
    }

    /// `workbench.action.openWalkthrough`: shows the page (opening its tab first when needed).
    pub(crate) fn open_welcome(&mut self, cx: &mut Context<Self>) {
        self.welcome_open = true;
        self.welcome_active = true;
        self.settings_active = false;
        cx.notify();
    }

    pub(crate) fn close_welcome(&mut self) {
        self.welcome_open = false;
        self.welcome_active = false;
    }

    /// The recent files the page lists: the editor history, files that still exist, most recent first.
    pub(crate) fn welcome_recent_files(&self) -> Vec<PathBuf> {
        self.editors.recent_files().into_iter().filter(|p| p.is_file()).take(RECENT_LIMIT).collect()
    }

    /// The "Show welcome page on startup" checkbox: it is `workbench.startupEditor` being
    /// `welcomePage`. Turning it off writes `none`; turning it on removes the user's value (the
    /// default is `welcomePage`).
    fn toggle_welcome_on_startup(&mut self, cx: &mut Context<Self>) {
        let on = startup_editor(AppConfig::get(cx).configuration()) == StartupEditor::WelcomePage;
        let value = on.then(|| serde_json::json!("none"));
        if let Err(e) = AppConfig::set_value(cx, "workbench.startupEditor", value) {
            self.error = Some(e);
        }
        cx.notify();
    }

    /// The tab in the tab strip.
    pub(crate) fn welcome_tab(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if !self.welcome_open {
            return None;
        }
        let t = &self.theme;
        let active = self.welcome_active && !self.settings_active;
        let fg = to_gpui(if active { t.tab_active_fg } else { t.tab_inactive_fg });
        let line = |d: Div, c| d.absolute().left_0().w_full().h(px(1.)).bg(to_gpui(c));
        let close = div()
            .id("welcome-tab-close")
            .flex_none()
            .w(px(20.))
            .h(px(20.))
            .rounded(px(5.))
            .flex()
            .items_center()
            .justify_center()
            .font_family(icons::FAMILY)
            .text_size(px(16.))
            .text_color(if active { fg } else { gpui_kit::transparent_black() })
            .group_hover("welcome-tab", |s| s.text_color(fg))
            .hover(|s| s.bg(to_gpui(t.list_hover_bg)))
            .child(icons::glyph(icons::CLOSE))
            .debug_selector(|| "welcome-tab-close".to_string())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.close_welcome();
                    cx.notify();
                }),
            );
        Some(
            div()
                .id("welcome-tab")
                .group("welcome-tab")
                .debug_selector(|| "welcome-tab".to_string())
                .relative()
                .flex_none()
                .h_full()
                .flex()
                .items_center()
                .gap(px(6.))
                .pl(px(12.))
                .pr(px(8.))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(fg)
                .bg(to_gpui(if active { t.tab_active_bg } else { t.tab_inactive_bg }))
                .child(if active { line(div().top_0(), t.tab_active_border_top) } else { line(div().bottom_0(), t.tab_border) })
                .child(div().absolute().right_0().top_0().h_full().w(px(1.)).bg(to_gpui(t.tab_border)))
                .child("Welcome")
                .child(close)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.welcome_active = true;
                        this.settings_active = false;
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(|this, _, _, cx| {
                        this.close_welcome();
                        cx.notify();
                    }),
                ),
        )
    }

    /// The page itself.
    pub(crate) fn welcome_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let fg = to_gpui(t.foreground);
        let dim = to_gpui(t.description_fg);
        let link: Rgba8 = t.text_link_fg;
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let on_startup = startup_editor(AppConfig::get(cx).configuration()) == StartupEditor::WelcomePage;

        // A link row: codicon, then the label, in the link colour.
        let action = |id: &'static str, icon: char, label: &'static str| {
            div()
                .id(id)
                .debug_selector(|| id.to_string())
                .flex()
                .items_center()
                .gap(px(8.))
                .py(px(3.))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(to_gpui(link))
                .child(div().font_family(icons::FAMILY).text_size(px(16.)).child(icons::glyph(icon)))
                .child(label)
        };
        let heading = |text: &'static str| div().mt(px(26.)).mb(px(6.)).text_size(px(19.5)).text_color(fg).child(text);

        let start = div()
            .flex()
            .flex_col()
            .child(heading("Start"))
            .child(action("welcome:open-file", icons::GO_TO_FILE, "Open File...").on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.show_quick_open(window, cx)),
            ));

        let recent = self.welcome_recent_files();
        let mut recent_list = div().flex().flex_col().child(heading("Recent"));
        if recent.is_empty() {
            recent_list = recent_list.child(div().text_color(dim).child("You have no recent files"));
        }
        for (i, path) in recent.iter().enumerate() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
            let dir = path.parent().map(|d| display_dir(d, home.as_deref())).unwrap_or_default();
            let target = path.clone();
            recent_list = recent_list.child(
                div()
                    .id(("welcome-recent", i))
                    .debug_selector(|| format!("welcome:recent:{i}"))
                    .flex()
                    .items_baseline()
                    .gap(px(10.))
                    .py(px(2.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .child(div().flex_none().text_color(to_gpui(link)).child(name))
                    .child(div().min_w_0().overflow_hidden().text_size(px(12.)).text_color(dim).child(dir))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.open(&target);
                            cx.notify();
                        }),
                    ),
            );
        }

        let checkbox = div()
            .id("welcome-startup-checkbox")
            .debug_selector(|| "welcome:startup-checkbox".to_string())
            .flex()
            .items_center()
            .gap(px(9.))
            .cursor_pointer()
            .text_size(px(13.))
            .text_color(dim)
            .child(
                div()
                    .flex_none()
                    .w(px(18.))
                    .h(px(18.))
                    .border_1()
                    .border_color(to_gpui(t.settings_checkbox_border))
                    .rounded(px(3.))
                    .bg(to_gpui(t.settings_checkbox_bg))
                    .text_color(to_gpui(t.settings_checkbox_fg))
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(icons::FAMILY)
                    .text_size(px(16.))
                    .when(on_startup, |d| d.child(icons::glyph(icons::CHECK))),
            )
            .child("Show welcome page on startup")
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.toggle_welcome_on_startup(cx)));

        div()
            .id("welcome-page")
            .debug_selector(|| "welcome-page".to_string())
            .size_full()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .bg(to_gpui(t.editor_bg))
            .text_color(fg)
            .child(
                div()
                    .w_full()
                    .max_w(px(760.))
                    .px(px(40.))
                    .py(px(48.))
                    .flex()
                    .flex_col()
                    .child(div().text_size(px(35.)).text_color(fg).child("rscode"))
                    .child(div().mt(px(8.)).text_size(px(26.)).text_color(dim).child("Editing, rewritten in Rust"))
                    .child(start)
                    .child(recent_list)
                    .child(div().mt(px(32.)).child(checkbox)),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::display_dir;

    #[test]
    fn display_dir_shortens_the_home_folder() {
        let home = Some(Path::new("/Users/me"));
        assert_eq!(display_dir(Path::new("/Users/me/dev/app"), home), "~/dev/app");
        assert_eq!(display_dir(Path::new("/Users/me"), home), "~");
        assert_eq!(display_dir(Path::new("/opt/x"), home), "/opt/x");
        assert_eq!(display_dir(Path::new("/opt/x"), None), "/opt/x");
    }
}
