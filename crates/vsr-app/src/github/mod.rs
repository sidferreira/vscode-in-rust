//! Pull requests (ADR-0113): a "Pull Requests" side-bar view, a read-only PR detail tab and the
//! `GitHub:` commands, all on top of the `gh` CLI through `vsr-github`. The crate blocks; every
//! call here runs on the background executor and its result comes back to the UI thread.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

mod create;
#[cfg(test)]
mod e2e_tests;
mod view;

use std::sync::Arc;

use gpui_kit::*;
use vsr_commands::CommandDescriptor as D;
use vsr_github::{GhError, GhRunner, Github, PrDetails, PrSummary, Snapshot, SystemRunner};

use crate::Workbench;
use crate::commands::{Handler, Scope};

actions!(rscode, [CreatePullRequest, CheckoutPullRequest, RefreshPullRequests, ShowPullRequests]);

pub const CREATE_ID: &str = "github.createPullRequest";
pub const CHECKOUT_ID: &str = "github.checkoutPullRequest";
pub const REFRESH_ID: &str = "github.refreshPullRequests";
pub const SHOW_ID: &str = "github.showPullRequests";

/// A line under the view or the PR tab about what the last action did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

/// What the detail tab shows for its pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrBody {
    Loading,
    Loaded(Box<PrDetails>),
    Failed(GhError),
}

/// The read-only detail tab (one instance: opening another pull request replaces it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrTab {
    pub number: u64,
    pub title: String,
    pub body: PrBody,
}

#[derive(Default)]
pub struct GithubState {
    /// `None` until first needed: then `gh` is looked up. Tests set a fake before anything runs.
    runner: Option<Arc<dyn GhRunner>>,
    /// The last load of the view's lists.
    pub load: Option<Result<Snapshot, GhError>>,
    pub loading: bool,
    /// Bumped per refresh, so a slow answer does not overwrite a newer one.
    generation: u64,
    pub pr: Option<PrTab>,
    pub pr_active: bool,
    pub notice: Option<Notice>,
    create: Option<create::CreateFlow>,
}

impl GithubState {
    /// Use this runner instead of the real `gh` (tests, embedders).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_runner(&mut self, runner: Arc<dyn GhRunner>) {
        self.runner = Some(runner);
    }

    pub fn pr_showing(&self) -> bool {
        self.pr.is_some() && self.pr_active
    }

    fn client(&mut self, root: &std::path::Path) -> Github {
        let runner = self.runner.get_or_insert_with(|| Arc::new(SystemRunner::locate())).clone();
        Github::new(runner, root.to_path_buf())
    }
}

/// The `GitHub:` commands in the Command Palette.
pub fn register_commands(r: &mut vsr_commands::CommandsRegistry<Handler>) {
    let wb = |a: Box<dyn Action>| Handler::Action { action: a, scope: Scope::Workbench };
    r.register(D::new(CREATE_ID, "Create Pull Request").category("GitHub").f1(), wb(Box::new(CreatePullRequest)));
    r.register(D::new(CHECKOUT_ID, "Checkout Pull Request").category("GitHub").f1(), wb(Box::new(CheckoutPullRequest)));
    r.register(D::new(REFRESH_ID, "Refresh Pull Requests").category("GitHub").f1(), wb(Box::new(RefreshPullRequests)));
    r.register(D::new(SHOW_ID, "Show Pull Requests").category("View").f1(), wb(Box::new(ShowPullRequests)));
}

impl Workbench {
    /// Attach the action handlers to the workbench root.
    pub(crate) fn bind_github_actions(el: Div, cx: &mut Context<Self>) -> Div {
        el.on_action(cx.listener(|this, _: &CreatePullRequest, window, cx| this.github_create(window, cx)))
            .on_action(cx.listener(|this, _: &CheckoutPullRequest, window, cx| this.github_checkout_pick(window, cx)))
            .on_action(cx.listener(|this, _: &RefreshPullRequests, _, cx| this.github_refresh(cx)))
            .on_action(cx.listener(|this, _: &ShowPullRequests, window, cx| this.show_pull_requests(window, cx)))
    }

    pub(crate) fn github_client(&mut self) -> Github {
        let root = self.root.clone();
        self.github.client(&root)
    }

    /// Shows the side-bar view (loading it the first time).
    pub(crate) fn show_pull_requests(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.view = crate::ActivityView::PullRequests;
        self.sidebar_visible = true;
        self.github_view_shown(cx);
    }

    /// The view came on screen: load the lists when nothing has been loaded yet.
    pub(crate) fn github_view_shown(&mut self, cx: &mut Context<Self>) {
        if self.github.load.is_none() && !self.github.loading {
            self.github_refresh(cx);
        }
        cx.notify();
    }

    /// Loads the repository and the three lists again.
    pub(crate) fn github_refresh(&mut self, cx: &mut Context<Self>) {
        self.github.loading = true;
        self.github.generation += 1;
        let generation = self.github.generation;
        let client = self.github_client();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { client.snapshot() }).await;
            this.update(cx, |this, cx| {
                if this.github.generation != generation {
                    return;
                }
                this.github.loading = false;
                this.github.load = Some(result);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Opens (or replaces) the detail tab for `pr` and loads its details.
    pub(crate) fn github_open_pr(&mut self, pr: &PrSummary, cx: &mut Context<Self>) {
        let number = pr.number;
        self.github.pr = Some(PrTab { number, title: pr.title.clone(), body: PrBody::Loading });
        self.github.pr_active = true;
        self.settings_active = false;
        self.welcome_active = false;
        self.github.notice = None;
        let client = self.github_client();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { client.pr_details(number) }).await;
            this.update(cx, |this, cx| {
                if let Some(tab) = this.github.pr.as_mut().filter(|t| t.number == number) {
                    tab.body = match result {
                        Ok(d) => {
                            tab.title = d.title.clone();
                            PrBody::Loaded(Box::new(d))
                        }
                        Err(e) => PrBody::Failed(e),
                    };
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn github_close_pr(&mut self) {
        self.github.pr = None;
        self.github.pr_active = false;
    }

    /// `gh pr checkout N`, then the status bar's branch follows.
    pub(crate) fn github_checkout(&mut self, number: u64, cx: &mut Context<Self>) {
        self.github.notice = Some(Notice { text: format!("Checking out #{number}..."), error: false });
        let client = self.github_client();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { client.checkout(number) }).await;
            this.update(cx, |this, cx| {
                this.github.notice = Some(match result {
                    Ok(()) => {
                        this.branch = crate::git_branch(&this.root);
                        Notice { text: format!("Checked out #{number}"), error: false }
                    }
                    Err(e) => Notice { text: format!("Checkout of #{number} failed: {}", e.headline()), error: true },
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// `gh pr view N --web`.
    pub(crate) fn github_open_in_browser(&mut self, number: u64, cx: &mut Context<Self>) {
        let client = self.github_client();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { client.open_in_browser(number) }).await;
            if let Err(e) = result {
                this.update(cx, |this, cx| {
                    this.github.notice = Some(Notice { text: format!("Could not open #{number}: {}", e.headline()), error: true });
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}
