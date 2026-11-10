//! Headless end-to-end tests of untitled text editors (ADR-0058, docs/testing.md tier 2). Our own
//! tests; the scenarios are in tests/e2e-scenarios/files.md ("Untitled text"). They run a real
//! `Workbench` in a headless window; no OS file dialog is opened: Save As takes the path it is given
//! (`save_untitled_to`, `save_untitled_in`) or the Explorer's name box, which the tests fill in.

use std::fs;
use std::path::{Path, PathBuf};

use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::Workbench;
use crate::config::AppConfig;
use crate::explorer::inline::InlineKind;
use crate::model::untitled_key;
use crate::session::backup::BackupStore;
use crate::session::runtime::TICK;
use crate::session::store::testing::unique_dir;
use crate::session::store::{SessionStorage, SessionStore};

struct Env {
    root: PathBuf,
    data: PathBuf,
    settings: PathBuf,
}

/// A folder with `a.rs` and `notes.txt`, a fresh user data directory and a settings file with `settings`.
fn env(name: &str, settings: &str) -> Env {
    let root = unique_dir(&format!("untitled-{name}-folder"));
    let data = unique_dir(&format!("untitled-{name}-userdata"));
    let settings_path = unique_dir(&format!("untitled-{name}-settings")).join("settings.json");
    fs::write(&settings_path, settings).unwrap();
    fs::write(root.join("a.rs"), "fn a() {}\n").unwrap();
    fs::write(root.join("notes.txt"), "notes\n").unwrap();
    Env { root, data, settings: settings_path }
}

impl Env {
    fn backups(&self) -> BackupStore {
        BackupStore::new(SessionStore::new(&self.data).backups_dir(&self.root))
    }
}

/// A workbench over the env's folder, with its settings and user data directory injected.
fn start<'a>(cx: &'a mut TestAppContext, env: &Env) -> (Entity<Workbench>, &'a mut VisualTestContext) {
    let (settings, data) = (env.settings.clone(), env.data.clone());
    cx.update(|cx| {
        if !cx.has_global::<SessionStorage>() {
            gpui_kit::init(cx);
            crate::icons::install(cx);
            crate::settings_view::install(cx);
            crate::editor_view::init(cx);
        }
        AppConfig::install(cx, Some(settings));
        cx.set_global(SessionStorage(Some(SessionStore::new(data))));
    });
    let root = env.root.clone();
    let (wb, vcx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    vcx.run_until_parked();
    (wb, vcx)
}

fn new_untitled(wb: &Entity<Workbench>, cx: &mut VisualTestContext) {
    wb.update_in(cx, |wb, window, cx| wb.new_untitled(window, cx));
    cx.run_until_parked();
}

/// The text typed into the active editor (the editor's own typing path, as the keys reach it).
fn type_in_active(wb: &Entity<Workbench>, cx: &mut VisualTestContext, text: &str) {
    wb.update(cx, |wb, cx| {
        let i = wb.editors.active.unwrap();
        wb.editors.open[i].editor.type_text(text);
        cx.notify();
    });
    cx.run_until_parked();
}

fn tick(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(TICK);
    cx.run_until_parked();
}

/// The key of the active editor, which must be an untitled text.
fn active_key(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> PathBuf {
    wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().path.clone())
}

fn with_untitled<T>(wb: &Entity<Workbench>, cx: &mut VisualTestContext, key: &Path, f: impl FnOnce(&crate::model::OpenEditor) -> T) -> T {
    wb.read_with(cx, |wb, _| f(&wb.editors.open[wb.editors.index_of(key).expect("the untitled text is open")]))
}

#[gpui::test]
fn new_untitled_text_is_an_empty_clean_tab_named_untitled_1(cx: &mut TestAppContext) {
    let env = env("new", "{}");
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    let key = active_key(&wb, cx);
    assert_eq!(key, untitled_key("Untitled-1"));
    assert_eq!(
        wb.read_with(cx, |wb, _| wb.editors.tab_labels(&env.root, crate::config::settings::LabelFormat::Default)[0].title.clone()),
        "Untitled-1"
    );
    assert!(!with_untitled(&wb, cx, &key, |e| e.is_dirty()));
}

#[gpui::test]
fn typing_makes_the_text_dirty_and_the_tab_takes_its_first_line(cx: &mut TestAppContext) {
    let env = env("typing", "{}");
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    let key = active_key(&wb, cx);
    type_in_active(&wb, cx, "todo list\nmilk");
    assert!(with_untitled(&wb, cx, &key, |e| e.is_dirty()));
    let title = wb.read_with(cx, |wb, _| wb.editors.title_of(wb.editors.active_editor().unwrap()));
    assert_eq!(title, "todo list");
    // Emptied again, the text is clean, and the tab is named by the name again.
    wb.update(cx, |wb, cx| {
        let i = wb.editors.active.unwrap();
        let full = wb.editors.open[i].buffer().full_range();
        wb.editors.open[i].editor.replace_ranges(&[(full, String::new())]);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(!with_untitled(&wb, cx, &key, |e| e.is_dirty()));
}

#[gpui::test]
fn label_format_name_keeps_the_resource_name_as_the_title(cx: &mut TestAppContext) {
    let env = env("label", r#"{ "workbench.editor.untitled.labelFormat": "name" }"#);
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    type_in_active(&wb, cx, "todo list");
    let title = wb.read_with(cx, |wb, _| wb.editors.title_of(wb.editors.active_editor().unwrap()));
    assert_eq!(title, "Untitled-1");
}

#[gpui::test]
fn files_default_language_is_the_language_of_a_new_untitled_text(cx: &mut TestAppContext) {
    let env = env("default-language", r#"{ "files.defaultLanguage": "rust" }"#);
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    let key = active_key(&wb, cx);
    assert_eq!(with_untitled(&wb, cx, &key, |e| e.language_id().to_string()), "rust");
}

#[gpui::test]
fn active_editor_language_is_the_default_when_the_setting_says_so(cx: &mut TestAppContext) {
    let env = env("active-language", r#"{ "files.defaultLanguage": "${activeEditorLanguage}" }"#);
    let (wb, cx) = start(cx, &env);
    wb.update(cx, |wb, _| wb.open(&env.root.join("a.rs")));
    cx.run_until_parked();
    new_untitled(&wb, cx);
    let key = active_key(&wb, cx);
    assert_eq!(with_untitled(&wb, cx, &key, |e| e.language_id().to_string()), "rust");
}

#[gpui::test]
fn save_as_writes_the_text_and_the_tab_becomes_the_file(cx: &mut TestAppContext) {
    let env = env("save-as", "{}");
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    let key = active_key(&wb, cx);
    type_in_active(&wb, cx, "fn main() {}\n");
    let target = env.root.join("main.rs");
    let t = target.clone();
    let k = key.clone();
    wb.update(cx, |wb, cx| wb.save_untitled_to(&k, &t, cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&target).unwrap(), "fn main() {}\n");
    wb.read_with(cx, |wb, _| {
        assert!(wb.editors.index_of(&key).is_none());
        let e = &wb.editors.open[wb.editors.index_of(&target).expect("the tab is the file now")];
        assert!(!e.is_dirty());
        assert!(!e.is_untitled());
        assert_eq!(e.language_id(), "rust", "the language is detected from the new name");
        assert_eq!(wb.editors.tab_labels(&env.root, crate::config::settings::LabelFormat::Default)[0].title, "main.rs");
    });
}

#[gpui::test]
fn save_as_refuses_a_name_that_is_already_a_file(cx: &mut TestAppContext) {
    let env = env("save-as-exists", "{}");
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    let key = active_key(&wb, cx);
    type_in_active(&wb, cx, "replacement\n");
    let k = key.clone();
    let root = env.root.clone();
    let result = wb.update(cx, |wb, cx| wb.save_untitled_in(&k, &root, "a.rs", cx));
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(env.root.join("a.rs")).unwrap(), "fn a() {}\n");
    assert!(with_untitled(&wb, cx, &key, |e| e.is_dirty()), "the untitled text stays dirty");
}

#[gpui::test]
fn save_on_an_untitled_text_opens_the_name_box_with_its_title(cx: &mut TestAppContext) {
    let env = env("save-box", "{}");
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    type_in_active(&wb, cx, "draft notes\nmore");
    wb.update_in(cx, |wb, window, cx| {
        let i = wb.editors.active.unwrap();
        wb.ask_save_untitled(i, window, cx);
    });
    cx.run_until_parked();
    let (kind, value) = wb.read_with(cx, |wb, cx| {
        let edit = wb.explorer.inline.as_ref().expect("the name box is open");
        (edit.kind, edit.value(cx))
    });
    assert_eq!(kind, InlineKind::SaveUntitled);
    assert_eq!(value, "draft notes");
}

#[gpui::test]
fn closing_a_dirty_untitled_text_asks_and_dont_save_drops_it(cx: &mut TestAppContext) {
    let env = env("dont-save", "{}");
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    let key = active_key(&wb, cx);
    type_in_active(&wb, cx, "scratch");
    tick(cx);
    tick(cx);
    assert!(env.backups().exists(&key), "its text is backed up while it is dirty");
    let k = key.clone();
    wb.update(cx, |wb, cx| wb.request_close(vec![k], cx));
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.session.close_prompt.is_some()), "a dirty untitled text asks first");
    wb.update_in(cx, |wb, window, cx| wb.close_prompt_dont_save(window, cx));
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.editors.index_of(&key).is_none()));
    assert!(!env.backups().exists(&key), "the backup is moved aside, not restored");
    assert!(SessionStore::new(&env.data).backups_dir(&env.root).join("discarded").is_dir());
}

#[gpui::test]
fn closing_a_clean_untitled_text_does_not_ask(cx: &mut TestAppContext) {
    let env = env("clean-close", "{}");
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    let key = active_key(&wb, cx);
    wb.update(cx, |wb, cx| wb.request_close(vec![key.clone()], cx));
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.session.close_prompt.is_none() && wb.editors.open.is_empty()));
}

#[gpui::test]
fn a_closed_untitled_number_is_reused(cx: &mut TestAppContext) {
    let env = env("reuse", "{}");
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    new_untitled(&wb, cx);
    assert_eq!(active_key(&wb, cx), untitled_key("Untitled-2"));
    let first = untitled_key("Untitled-1");
    wb.update(cx, |wb, cx| wb.request_close(vec![first], cx));
    cx.run_until_parked();
    new_untitled(&wb, cx);
    assert_eq!(active_key(&wb, cx), untitled_key("Untitled-1"));
}

#[gpui::test]
fn an_untitled_text_is_not_closed_as_a_missing_file(cx: &mut TestAppContext) {
    let env = env("missing", "{}");
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    let closed = wb.update(cx, |wb, _| wb.editors.close_missing(|_| false));
    assert!(closed.is_empty());
    assert_eq!(wb.read_with(cx, |wb, _| wb.editors.open.len()), 1);
}

#[gpui::test]
fn hot_exit_brings_the_untitled_text_back_as_an_untitled_text(cx: &mut TestAppContext) {
    let env = env("hot-exit", r#"{ "files.defaultLanguage": "rust" }"#);
    {
        let (wb, cx) = start(cx, &env);
        new_untitled(&wb, cx);
        type_in_active(&wb, cx, "draft\nnotes");
        tick(cx);
        tick(cx);
        wb.update(cx, |wb, cx| wb.persist_on_quit(cx));
    }
    let (wb, cx) = start(cx, &env);
    let key = untitled_key("Untitled-1");
    assert!(with_untitled(&wb, cx, &key, |e| e.is_dirty() && e.buffer().value() == "draft\nnotes"));
    assert_eq!(with_untitled(&wb, cx, &key, |e| e.language_id().to_string()), "rust", "the backup keeps the language");
}

#[gpui::test]
fn cmd_n_and_cmd_s_reach_the_untitled_commands(cx: &mut TestAppContext) {
    let env = env("keys", "{}");
    let (wb, cx) = start(cx, &env);
    cx.update(|_window, cx| crate::commands::init(cx));
    cx.simulate_keystrokes("cmd-n");
    cx.run_until_parked();
    assert_eq!(active_key(&wb, cx), untitled_key("Untitled-1"));
    type_in_active(&wb, cx, "hello");
    cx.simulate_keystrokes("cmd-s");
    cx.run_until_parked();
    let kind = wb.read_with(cx, |wb, _| wb.explorer.inline.as_ref().map(|e| e.kind));
    assert_eq!(kind, Some(InlineKind::SaveUntitled));
}

#[gpui::test]
fn startup_editor_new_untitled_file_opens_an_untitled_text(cx: &mut TestAppContext) {
    let env = env("startup", r#"{ "workbench.startupEditor": "newUntitledFile" }"#);
    let (wb, cx) = start(cx, &env);
    assert_eq!(active_key(&wb, cx), untitled_key("Untitled-1"));
    assert!(!with_untitled(&wb, cx, &untitled_key("Untitled-1"), |e| e.is_dirty()));
}

#[gpui::test]
fn files_eol_is_the_line_break_of_a_new_untitled_text(cx: &mut TestAppContext) {
    let env = env("eol", r#"{ "files.eol": "\r\n" }"#);
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    type_in_active(&wb, cx, "one");
    wb.update(cx, |wb, _| {
        let i = wb.editors.active.unwrap();
        wb.editors.open[i].enter();
    });
    type_in_active(&wb, cx, "two");
    let key = active_key(&wb, cx);
    let target = env.root.join("eol.txt");
    let (k, t) = (key.clone(), target.clone());
    wb.update(cx, |wb, cx| wb.save_untitled_to(&k, &t, cx)).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"one\r\ntwo");
}

#[gpui::test]
fn closing_a_dirty_untitled_text_saves_as_then_closes_its_tab(cx: &mut TestAppContext) {
    let env = env("save-then-close", "{}");
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    let key = active_key(&wb, cx);
    type_in_active(&wb, cx, "keep me");
    wb.update(cx, |wb, cx| wb.request_close(vec![key.clone()], cx));
    cx.run_until_parked();
    wb.update_in(cx, |wb, window, cx| wb.close_prompt_save(window, cx));
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.editors.index_of(&key).is_some()), "the tab stays until the file is written");
    let target = env.root.join("kept.txt");
    let (k, t) = (key.clone(), target.clone());
    wb.update(cx, |wb, cx| wb.save_untitled_to(&k, &t, cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep me");
    assert!(wb.read_with(cx, |wb, _| wb.editors.index_of(&target).is_none()), "the saved tab closes");
}

#[gpui::test]
fn a_plain_save_as_keeps_the_tab_open(cx: &mut TestAppContext) {
    let env = env("plain-save-as", "{}");
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    let key = active_key(&wb, cx);
    type_in_active(&wb, cx, "stay");
    wb.update_in(cx, |wb, window, cx| {
        let i = wb.editors.active.unwrap();
        wb.ask_save_untitled(i, window, cx);
    });
    let target = env.root.join("stay.txt");
    let (k, t) = (key.clone(), target.clone());
    wb.update(cx, |wb, cx| wb.save_untitled_to(&k, &t, cx)).unwrap();
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.editors.index_of(&target).is_some()));
}

#[gpui::test]
fn a_restored_untitled_text_keeps_its_tab_position(cx: &mut TestAppContext) {
    let env = env("position", "{}");
    {
        let (wb, cx) = start(cx, &env);
        wb.update(cx, |wb, _| wb.open(&env.root.join("a.rs")));
        cx.run_until_parked();
        new_untitled(&wb, cx);
        type_in_active(&wb, cx, "draft");
        wb.update(cx, |wb, _| wb.open(&env.root.join("notes.txt")));
        cx.run_until_parked();
        tick(cx);
        tick(cx);
        wb.update(cx, |wb, cx| wb.persist_on_quit(cx));
    }
    let (wb, cx) = start(cx, &env);
    let order: Vec<PathBuf> = wb.read_with(cx, |wb, _| wb.editors.open.iter().map(|e| e.path.clone()).collect());
    assert_eq!(order, vec![env.root.join("a.rs"), untitled_key("Untitled-1"), env.root.join("notes.txt")]);
}

#[gpui::test]
fn restore_applies_the_editor_limit_to_the_saved_tabs(cx: &mut TestAppContext) {
    let env = env("limit", "{}");
    fs::write(env.root.join("c.txt"), "c\n").unwrap();
    {
        let (wb, cx) = start(cx, &env);
        for name in ["a.rs", "notes.txt", "c.txt"] {
            let path = env.root.join(name);
            wb.update(cx, |wb, _| wb.open(&path));
            cx.run_until_parked();
        }
        assert_eq!(wb.read_with(cx, |wb, _| wb.editors.open.len()), 3);
        tick(cx);
        wb.update(cx, |wb, cx| wb.persist_on_quit(cx));
    }
    fs::write(&env.settings, r#"{ "workbench.editor.limit.enabled": true, "workbench.editor.limit.value": 2 }"#).unwrap();
    let (wb, cx) = start(cx, &env);
    let open: Vec<PathBuf> = wb.read_with(cx, |wb, _| wb.editors.open.iter().map(|e| e.path.clone()).collect());
    assert_eq!(open, vec![env.root.join("notes.txt"), env.root.join("c.txt")], "the least recently used tab goes, the active one stays");
    let active = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().path.clone());
    assert_eq!(active, env.root.join("c.txt"));
}

#[gpui::test]
fn save_as_starts_in_the_folder_of_the_active_file(cx: &mut TestAppContext) {
    let env = env("save-folder", "{}");
    fs::create_dir_all(env.root.join("sub")).unwrap();
    fs::write(env.root.join("sub").join("b.txt"), "b\n").unwrap();
    let (wb, cx) = start(cx, &env);
    let sub = env.root.join("sub").join("b.txt");
    wb.update(cx, |wb, _| wb.open(&sub));
    cx.run_until_parked();
    new_untitled(&wb, cx);
    type_in_active(&wb, cx, "hello");
    wb.update_in(cx, |wb, window, cx| {
        let i = wb.editors.active.unwrap();
        wb.ask_save_untitled(i, window, cx);
    });
    cx.run_until_parked();
    let parent = wb.read_with(cx, |wb, _| wb.explorer.inline.as_ref().unwrap().parent.clone());
    assert_eq!(parent, env.root.join("sub"));
}

#[gpui::test]
fn save_as_suggests_the_extension_of_the_language(cx: &mut TestAppContext) {
    let env = env("save-extension", r#"{ "files.defaultLanguage": "rust" }"#);
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    type_in_active(&wb, cx, "notes");
    wb.update_in(cx, |wb, window, cx| {
        let i = wb.editors.active.unwrap();
        wb.ask_save_untitled(i, window, cx);
    });
    cx.run_until_parked();
    let value = wb.read_with(cx, |wb, cx| wb.explorer.inline.as_ref().unwrap().value(cx));
    assert_eq!(value, "notes.rs");
}

#[gpui::test]
fn blurring_the_save_as_box_does_not_save(cx: &mut TestAppContext) {
    let env = env("save-blur", "{}");
    let (wb, cx) = start(cx, &env);
    new_untitled(&wb, cx);
    let key = active_key(&wb, cx);
    type_in_active(&wb, cx, "hello");
    wb.update_in(cx, |wb, window, cx| {
        let i = wb.editors.active.unwrap();
        wb.ask_save_untitled(i, window, cx);
    });
    cx.run_until_parked();
    // The name box has the focus; losing it runs the focus-out handler. Headless focus-out is not
    // delivered to the box, so the handler is called directly (explorer/inline.rs).
    wb.update_in(cx, |wb, window, cx| wb.blur_inline(window, cx));
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.explorer.inline.is_none()), "the box closes");
    assert!(!env.root.join("hello").exists(), "a blur writes nothing");
    assert!(with_untitled(&wb, cx, &key, |e| e.is_dirty()), "the text stays an untitled text");
}
