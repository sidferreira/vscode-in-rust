//! `GitHub: Create Pull Request` and `GitHub: Checkout Pull Request`: Quick Pick steps over the
//! existing picker. Nothing is pushed unless the user confirms the push step.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

use gpui_kit::*;
use vsr_github::{BranchInfo, NewPr, PrFilter, PrSummary};
use vsr_quickinput::pick::{PickItem, PickRow, filter_items};

use super::Notice;
use crate::Workbench;
use crate::config::AppConfig;
use crate::quick_input::{PickResult, QuickPick, QuickPickEvent};

/// Where the create flow is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateStep {
    Push,
    Title,
    Body,
    Base,
    Draft,
}

#[derive(Debug, Clone)]
pub struct CreateFlow {
    /// The invocation that made this flow ([`super::GithubState`]'s `create_gen` then).
    pub generation: u64,
    pub info: BranchInfo,
    pub step: CreateStep,
    pub title: String,
    pub body: String,
    pub base: String,
}

/// Rows of a choice step: label and value.
pub(super) fn choices(rows: &[(&str, &str)]) -> Vec<PickItem> {
    rows.iter()
        .enumerate()
        .map(|(i, (label, value))| {
            let mut item = PickItem::new(i, *label);
            item.value = Some((*value).into());
            item
        })
        .collect()
}

impl Workbench {
    /// `GitHub: Create Pull Request`: read the branch and the default base in the background, then
    /// ask (push, title, body, base, draft) and run `gh pr create`.
    pub(crate) fn github_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let client = self.github_client();
        self.github.create_gen += 1;
        let generation = self.github.create_gen;
        self.github.create = None;
        self.github.notice = Some(Notice { text: "Preparing a pull request...".into(), error: false });
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let info = client.branch_info()?;
                    if info.is_detached() {
                        return Ok::<_, vsr_github::GhError>((info, String::new()));
                    }
                    let repo = client.repo()?;
                    Ok((info, repo.default_branch().to_string()))
                })
                .await;
            this.update_in(cx, |this, window, cx| this.github_create_prepared(generation, result, window, cx)).ok();
        })
        .detach();
        cx.notify();
    }

    /// The branch and the default base arrived for the invocation `generation`; an older one than
    /// the latest is dropped.
    pub(super) fn github_create_prepared(
        &mut self,
        generation: u64,
        result: Result<(BranchInfo, String), vsr_github::GhError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if generation != self.github.create_gen {
            return;
        }
        match result {
            Err(e) => {
                self.github.notice = Some(Notice { text: format!("Cannot create a pull request: {}", e.headline()), error: true });
                cx.notify();
            }
            Ok((info, _)) if info.is_detached() => {
                // Never push or pass `--head HEAD`.
                self.github.notice = Some(Notice { text: "Cannot create a pull request: Check out a branch before creating a pull request.".into(), error: true });
                cx.notify();
            }
            Ok((info, base)) => {
                self.github.notice = None;
                let step = if info.has_upstream { CreateStep::Title } else { CreateStep::Push };
                let title = info.last_commit_subject.clone();
                self.github.create = Some(CreateFlow { generation, info, step, title, body: String::new(), base });
                self.github_create_ask(window, cx);
            }
        }
    }

    /// Shows the picker of the current step.
    fn github_create_ask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(flow) = self.github.create.clone() else { return };
        let (placeholder, initial, choices): (String, String, Option<Vec<PickItem>>) = match flow.step {
            CreateStep::Push => (
                format!("Branch '{}' has no upstream. Push it to origin first?", flow.info.branch),
                String::new(),
                Some(choices(&[("Yes, run `git push -u origin HEAD`", "yes"), ("No, cancel", "no")])),
            ),
            CreateStep::Title => ("Pull request title".into(), flow.title.clone(), None),
            CreateStep::Body => ("Pull request description (optional)".into(), flow.body.clone(), None),
            CreateStep::Base => ("Base branch".into(), flow.base.clone(), None),
            CreateStep::Draft => ("Create as a draft?".into(), String::new(), Some(choices(&[("No, ready for review", "ready"), ("Yes, create as a draft", "draft")]))),
        };
        self.github_pick(placeholder, initial, choices, window, cx, |this, item, query, window, cx| {
            this.github_create_accept(item.and_then(|i| i.value), query, window, cx)
        });
    }

    /// An answer to the current step.
    fn github_create_accept(&mut self, value: Option<String>, query: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut flow) = self.github.create.take() else { return };
        match flow.step {
            CreateStep::Push => {
                if value.as_deref() != Some("yes") {
                    self.github.notice = Some(Notice { text: "Pull request not created: the branch was not pushed.".into(), error: false });
                    cx.notify();
                    return;
                }
                self.github.notice = Some(Notice { text: format!("Pushing '{}'...", flow.info.branch), error: false });
                let client = self.github_client();
                cx.spawn_in(window, async move |this, cx| {
                    let result = cx.background_executor().spawn(async move { client.push_upstream() }).await;
                    this.update_in(cx, |this, window, cx| match result {
                        _ if flow.generation != this.github.create_gen => {}
                        Ok(()) => {
                            this.github.notice = None;
                            flow.info.has_upstream = true;
                            flow.step = CreateStep::Title;
                            this.github.create = Some(flow);
                            this.github_create_ask(window, cx);
                        }
                        Err(e) => {
                            this.github.notice = Some(Notice { text: format!("Push failed: {}", e.headline()), error: true });
                            cx.notify();
                        }
                    })
                    .ok();
                })
                .detach();
                cx.notify();
            }
            CreateStep::Title => {
                let title = query.trim().to_string();
                if !title.is_empty() {
                    flow.title = title;
                    flow.step = CreateStep::Body;
                }
                self.github.create = Some(flow);
                self.github_create_ask(window, cx);
            }
            CreateStep::Body => {
                flow.body = query;
                flow.step = CreateStep::Base;
                self.github.create = Some(flow);
                self.github_create_ask(window, cx);
            }
            CreateStep::Base => {
                let base = query.trim().to_string();
                if !base.is_empty() {
                    flow.base = base;
                    flow.step = CreateStep::Draft;
                }
                self.github.create = Some(flow);
                self.github_create_ask(window, cx);
            }
            CreateStep::Draft => {
                let draft = value.as_deref() == Some("draft");
                self.github_create_run(
                    NewPr { title: flow.title, body: flow.body, base: flow.base, head: flow.info.branch, draft },
                    cx,
                );
            }
        }
    }

    fn github_create_run(&mut self, pr: NewPr, cx: &mut Context<Self>) {
        self.github.notice = Some(Notice { text: "Creating the pull request...".into(), error: false });
        let client = self.github_client();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { client.create_pr(&pr) }).await;
            this.update(cx, |this, cx| {
                this.github.notice = Some(match result {
                    Ok(url) => Notice { text: format!("Pull request created: {url}"), error: false },
                    Err(e) => Notice { text: format!("Could not create the pull request: {}", e.headline()), error: true },
                });
                this.github_refresh(cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// `GitHub: Checkout Pull Request`: a Quick Pick of the open pull requests.
    pub(crate) fn github_checkout_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let client = self.github_client();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move { client.list_prs(PrFilter::Open) }).await;
            this.update_in(cx, |this, window, cx| match result {
                Err(e) => {
                    this.github.notice = Some(Notice { text: format!("Cannot list pull requests: {}", e.headline()), error: true });
                    cx.notify();
                }
                Ok(prs) if prs.is_empty() => {
                    this.github.notice = Some(Notice { text: "There are no open pull requests.".into(), error: false });
                    cx.notify();
                }
                Ok(prs) => {
                    let items: Vec<PickItem> = prs.iter().enumerate().map(|(i, pr)| pr_item(i, pr)).collect();
                    this.github_pick("Select a pull request to check out".into(), String::new(), Some(items), window, cx, move |this, item, _q, window, cx| {
                        if let Some(number) = item.and_then(|i| i.value).and_then(|v| v.parse().ok()) {
                            this.github_checkout(number, window, cx);
                        }
                    });
                }
            })
            .ok();
        })
        .detach();
    }

    /// Closes the picker of a step without ending the flow (the next step, or the answer, follows).
    fn github_hide_picker(&mut self, restore_focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.github.keep_create = true;
        self.hide_quick_open(restore_focus, window, cx);
        self.github.keep_create = false;
    }

    /// A Quick Pick: `choices` (filtered by the typed text), or a text step when `None`. `accept`
    /// gets the chosen item and the typed text, with the picker already closed.
    pub(super) fn github_pick(
        &mut self,
        placeholder: String,
        initial: String,
        choices: Option<Vec<PickItem>>,
        window: &mut Window,
        cx: &mut Context<Self>,
        accept: impl Fn(&mut Workbench, Option<PickItem>, String, &mut Window, &mut Context<Workbench>) + 'static,
    ) {
        if self.quick.pick.is_some() {
            self.github_hide_picker(false, window, cx);
        }
        let hint = placeholder.clone();
        let source = move |query: &str, _cx: &mut App| -> PickResult {
            match &choices {
                Some(items) if query.trim().is_empty() => PickResult::Ready(items.iter().cloned().map(PickRow::Item).collect()),
                Some(items) => PickResult::Ready(filter_items(items, query)),
                None => {
                    let mut item = PickItem::new(0, if query.is_empty() { "Press Enter to confirm".to_string() } else { query.to_string() });
                    item.description = Some(hint.clone());
                    PickResult::Ready(vec![PickRow::Item(item)])
                }
            }
        };
        let close_on_focus_lost = crate::quick_open::QuickOpenSettings::from_configuration(AppConfig::get(cx).configuration()).close_on_focus_lost;
        self.quick.return_focus = window.focused(cx);
        self.quick.provider = String::from("github");
        let select_from = (!initial.is_empty()).then_some(0);
        let pick = cx.new(|cx| QuickPick::new(&placeholder, &initial, select_from, close_on_focus_lost, source, window, cx));
        self.quick._sub = Some(cx.subscribe_in(&pick, window, move |this, _, ev: &QuickPickEvent, window, cx| match ev {
            QuickPickEvent::Accept { item, query, .. } => {
                this.github_hide_picker(true, window, cx);
                accept(this, Some(item.clone()), query.clone(), window, cx);
            }
            QuickPickEvent::Dismiss => {
                this.github.create = None;
                this.hide_quick_open(true, window, cx);
            }
            _ => {}
        }));
        self.quick.pick = Some(pick);
        cx.notify();
    }
}

fn pr_item(id: usize, pr: &PrSummary) -> PickItem {
    let mut item = PickItem::new(id, format!("#{} {}", pr.number, pr.title));
    item.description = Some(format!("{} · {}", pr.author_login(), pr.head_ref_name));
    item.value = Some(pr.number.to_string());
    item
}
