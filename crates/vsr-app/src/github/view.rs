//! The Pull Requests side-bar view and the PR detail tab.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use vsr_github::{CheckOutcome, GhError, PrDetails, PrSummary};

use super::{PrBody, PrTab};
use crate::config::settings::TabActions;
use crate::ui::to_gpui;
use crate::{Workbench, icons};

/// Which of the three lists a row belongs to (selectors and ids).
#[derive(Clone, Copy)]
enum Section {
    Open,
    Mine,
    Review,
}

impl Section {
    fn key(self) -> &'static str {
        match self {
            Section::Open => "open",
            Section::Mine => "mine",
            Section::Review => "review",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Section::Open => "Open",
            Section::Mine => "Created by me",
            Section::Review => "Review requested",
        }
    }
}

/// What the view says when `gh` cannot serve it.
pub fn error_lines(e: &GhError) -> (String, String) {
    match e {
        GhError::GhMissing => (
            "Install GitHub CLI".into(),
            "Pull requests use the `gh` command. Install it from https://cli.github.com (or `brew install gh`), then refresh.".into(),
        ),
        GhError::NotSignedIn(_) => (
            "Run `gh auth login`".into(),
            "Sign in to GitHub with the `gh` command in a terminal, then refresh.".into(),
        ),
        GhError::NotGitHubRepo(_) => (
            "This folder is not a GitHub repository".into(),
            "Open a folder whose git remote points to GitHub, then refresh.".into(),
        ),
        other => ("Could not load pull requests".into(), other.to_string()),
    }
}

impl Workbench {
    /// The view's body (the side bar draws the heading above it).
    pub(crate) fn github_side_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let dim = to_gpui(t.description_fg);
        let button = |id: &'static str, glyph: char, tip: &'static str| {
            div()
                .id(id)
                .debug_selector(|| id.to_string())
                .flex_none()
                .w(px(22.))
                .h(px(22.))
                .rounded(px(5.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .font_family(icons::FAMILY)
                .text_size(px(16.))
                .hover(|s| s.bg(to_gpui(t.toolbar_hover_bg)))
                .child(icons::glyph(glyph))
                .tooltip(move |w, cx| Tooltip::new(tip).build(w, cx))
        };
        let toolbar = div().flex_none().h(px(22.)).px(px(12.)).flex().items_center().justify_between().text_size(px(11.)).text_color(dim).child(
            div()
                .debug_selector(|| "github:repo".to_string())
                .child(match &self.github.load {
                    Some(Ok(s)) => s.repo.name_with_owner.clone(),
                    _ => String::new(),
                }),
        )
        .child(
            button("github:refresh", icons::REFRESH, "Refresh Pull Requests")
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.github_refresh(cx))),
        );

        let mut body = div().id("github-body").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().pb(px(8.));
        match &self.github.load {
            None if self.github.loading => body = body.child(message("github:loading", "Loading pull requests...", dim)),
            None => {}
            Some(Err(e)) => {
                let (headline, detail) = error_lines(e);
                body = body.child(
                    div()
                        .debug_selector(|| "github:error".to_string())
                        .px(px(20.))
                        .py(px(8.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .text_size(px(13.))
                        .child(div().child(headline))
                        .child(div().text_size(px(12.)).text_color(dim).child(detail)),
                );
            }
            Some(Ok(snapshot)) => {
                for (section, prs) in [
                    (Section::Open, &snapshot.open),
                    (Section::Mine, &snapshot.created_by_me),
                    (Section::Review, &snapshot.review_requested),
                ] {
                    body = body.child(self.github_section(section, prs, cx));
                }
            }
        }
        if let Some(n) = &self.github.notice {
            let color = if n.error { to_gpui(t.validation_error_border) } else { dim };
            body = body.child(
                div().debug_selector(|| "github:notice".to_string()).px(px(20.)).py(px(6.)).text_size(px(12.)).text_color(color).child(n.text.clone()),
            );
        }
        div().flex_1().min_h_0().flex().flex_col().debug_selector(|| "github:view".to_string()).child(toolbar).child(body).into_any_element()
    }

    fn github_section(&self, section: Section, prs: &[PrSummary], cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let key = section.key();
        let mut list = div().flex().flex_col().child(
            div()
                .debug_selector(move || format!("github:section:{key}"))
                .h(px(22.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(4.))
                .text_size(px(11.))
                .font_weight(FontWeight::BOLD)
                .child(div().font_family(icons::FAMILY).text_size(px(16.)).child(icons::glyph(icons::CHEVRON_DOWN)))
                .child(format!("{} ({})", section.title().to_uppercase(), prs.len())),
        );
        if prs.is_empty() {
            list = list.child(message("github:empty", "No pull requests", to_gpui(t.description_fg)));
        }
        for (i, pr) in prs.iter().enumerate() {
            let number = pr.number;
            let target = pr.clone();
            let draft = if pr.is_draft { " · draft" } else { "" };
            let active = self.github.pr_showing() && self.github.pr.as_ref().is_some_and(|p| p.number == number);
            list = list.child(
                div()
                    .id((key, i))
                    .debug_selector(move || format!("github:pr:{key}:{number}"))
                    .flex_none()
                    .px(px(24.))
                    .py(px(3.))
                    .flex()
                    .flex_col()
                    .cursor_pointer()
                    .when(active, |d| d.bg(to_gpui(t.list_active_bg)))
                    .when(!active, |d| d.hover(|s| s.bg(to_gpui(t.list_hover_bg))))
                    .child(div().text_size(px(13.)).overflow_hidden().whitespace_nowrap().child(format!("#{number} {}", pr.title)))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(to_gpui(t.description_fg))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(format!("{} · {}{draft}", pr.author_login(), pr.head_ref_name)),
                    )
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| this.github_open_pr(&target, cx))),
            );
        }
        list
    }

    /// The tab in the tab strip.
    pub(crate) fn github_pr_tab(&self, cx: &mut Context<Self>, actions: TabActions) -> Option<impl IntoElement> {
        let pr = self.github.pr.as_ref()?;
        let t = &self.theme;
        let active = self.github.pr_active && !self.settings_active;
        let fg = to_gpui(if active { t.tab_active_fg } else { t.tab_inactive_fg });
        let line = |d: Div, c| d.absolute().left_0().w_full().h(px(1.)).bg(to_gpui(c));
        let close = div()
            .id("github-tab-close")
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
            .group_hover("github-tab", |s| s.text_color(fg))
            .hover(|s| s.bg(to_gpui(t.list_hover_bg)))
            .child(icons::glyph(icons::CLOSE))
            .debug_selector(|| "github-tab-close".to_string())
            .when(actions.clickable, |d| {
                d.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.github_close_pr();
                        cx.notify();
                    }),
                )
            });
        let (close_left, close_right) = actions.place(close);
        let (pad_left, pad_right) = actions.padding();
        Some(
            div()
                .id("github-tab")
                .group("github-tab")
                .debug_selector(|| "github-tab".to_string())
                .relative()
                .flex_none()
                .h_full()
                .flex()
                .items_center()
                .gap(px(6.))
                .pl(px(pad_left))
                .pr(px(pad_right))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(fg)
                .bg(to_gpui(if active { t.tab_active_bg } else { t.tab_inactive_bg }))
                .child(if active { line(div().top_0(), t.tab_active_border_top) } else { line(div().bottom_0(), t.tab_border) })
                .child(div().absolute().right_0().top_0().h_full().w(px(1.)).bg(to_gpui(t.tab_border)))
                .children(close_left)
                .child(div().font_family(icons::FAMILY).text_size(px(16.)).child(icons::glyph(icons::GIT_PULL_REQUEST)))
                .child(format!("#{} {}", pr.number, pr.title))
                .children(close_right)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.github.pr_active = true;
                        this.settings_active = false;
                        this.welcome_active = false;
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(|this, _, _, cx| {
                        this.github_close_pr();
                        cx.notify();
                    }),
                ),
        )
    }

    /// The read-only detail page.
    pub(crate) fn github_pr_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let fg = to_gpui(t.foreground);
        let dim = to_gpui(t.description_fg);
        let page = |content: Div| {
            div()
                .id("github-pr-page")
                .debug_selector(|| "github-pr-page".to_string())
                .size_full()
                .overflow_y_scroll()
                .flex()
                .justify_center()
                .bg(to_gpui(t.editor_bg))
                .text_color(fg)
                .child(content.w_full().max_w(px(860.)).px(px(40.)).py(px(32.)).flex().flex_col().gap(px(10.)))
        };
        let Some(PrTab { number, title, body }) = self.github.pr.clone() else {
            return page(div()).into_any_element();
        };
        match body {
            PrBody::Loading => page(
                div()
                    .child(div().text_size(px(22.)).child(format!("{title} #{number}")))
                    .child(div().debug_selector(|| "github:pr-loading".to_string()).text_color(dim).child("Loading...")),
            )
            .into_any_element(),
            PrBody::Failed(e) => {
                let (headline, detail) = error_lines(&e);
                page(
                    div()
                        .child(div().text_size(px(22.)).child(format!("{title} #{number}")))
                        .child(div().debug_selector(|| "github:pr-error".to_string()).child(headline))
                        .child(div().text_size(px(12.)).text_color(dim).child(detail)),
                )
                .into_any_element()
            }
            PrBody::Loaded(d) => page(self.github_pr_content(&d, cx)).into_any_element(),
        }
    }

    fn github_pr_content(&self, d: &PrDetails, cx: &mut Context<Self>) -> Div {
        let t = &self.theme;
        let dim = to_gpui(t.description_fg);
        let number = d.number;
        let button = |id: &'static str, label: &'static str, primary: bool| {
            let (bg, fg, hover) = if primary {
                (t.button_bg, t.button_fg, t.button_hover_bg)
            } else {
                (t.button_secondary_bg, t.button_secondary_fg, t.button_secondary_hover_bg)
            };
            div()
                .id(id)
                .debug_selector(|| id.to_string())
                .px(px(12.))
                .py(px(4.))
                .rounded(px(2.))
                .cursor_pointer()
                .text_size(px(13.))
                .bg(to_gpui(bg))
                .text_color(to_gpui(fg))
                .hover(|s| s.bg(to_gpui(hover)))
                .child(label)
        };
        let state = if d.is_draft { format!("{} (draft)", d.state.label()) } else { d.state.label().to_string() };
        let checks = d.checks();
        let mut content = div()
            .child(div().debug_selector(|| "github:pr-title".to_string()).text_size(px(22.)).child(format!("{} #{}", d.title, d.number)))
            .child(
                div()
                    .debug_selector(|| "github:pr-meta".to_string())
                    .text_size(px(13.))
                    .text_color(dim)
                    .child(format!("{state} · {} · {} ← {}", d.author_login(), d.base_ref_name, d.head_ref_name)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        button("github:pr-checkout", "Checkout", true)
                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| this.github_checkout(number, cx))),
                    )
                    .child(
                        button("github:pr-open-web", "Open on GitHub", false)
                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| this.github_open_in_browser(number, cx))),
                    ),
            );
        if let Some(n) = &self.github.notice {
            let color = if n.error { to_gpui(t.validation_error_border) } else { dim };
            content = content.child(div().debug_selector(|| "github:pr-notice".to_string()).text_size(px(12.)).text_color(color).child(n.text.clone()));
        }
        content = content
            .child(heading("Description"))
            .child(
                div()
                    .debug_selector(|| "github:pr-body".to_string())
                    .text_size(px(13.))
                    .whitespace_normal()
                    .child(if d.body.trim().is_empty() { "No description provided.".to_string() } else { d.body.clone() }),
            )
            .child(heading("Checks"))
            .child(div().debug_selector(|| "github:pr-checks".to_string()).text_size(px(13.)).child(checks.text()));
        for c in &d.status_check_rollup {
            let mark = match c.outcome() {
                CheckOutcome::Passed => "passed",
                CheckOutcome::Failed => "failed",
                CheckOutcome::Pending => "pending",
                CheckOutcome::Skipped => "skipped",
            };
            content = content.child(div().text_size(px(12.)).text_color(dim).child(format!("{}: {mark}", c.label())));
        }
        content = content.child(heading(&format!("Files changed ({})", d.files.len())));
        for (i, f) in d.files.iter().enumerate() {
            content = content.child(
                div()
                    .debug_selector(move || format!("github:pr-file:{i}"))
                    .flex()
                    .gap(px(12.))
                    .text_size(px(12.))
                    .child(div().flex_1().min_w_0().overflow_hidden().child(f.path.clone()))
                    .child(div().flex_none().text_color(dim).child(format!("+{} -{}", f.additions, f.deletions))),
            );
        }
        content
    }
}

fn message(id: &'static str, text: &'static str, color: Hsla) -> impl IntoElement {
    div().debug_selector(|| id.to_string()).px(px(24.)).py(px(4.)).text_size(px(12.)).text_color(color).child(text)
}

fn heading(text: &str) -> Div {
    div().mt(px(14.)).text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(text.to_string())
}
