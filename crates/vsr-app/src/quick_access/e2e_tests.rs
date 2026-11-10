//! Headless E2E tests for the Command Palette, Go to Line and the Quick Access prefixes
//! (docs/testing.md tier 2; scenarios in tests/e2e-scenarios/quickaccess.md): real key dispatch
//! over a temp workspace with the in-memory configuration (never the real settings file).

use std::fs;
use std::path::PathBuf;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use vsr_quickinput::pick::PickRow;

use crate::Workbench;
use crate::commands::Commands;
use crate::config::AppConfig;
use crate::quick_input::QuickPick;

fn workspace(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-e2e-palette-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
    let hundred: String = (1..=100).map(|i| format!("line {i}\n")).collect();
    fs::write(dir.join("src/long.txt"), hundred).unwrap();
    fs::write(dir.join("src/tabs.txt"), "\tindented\nplain\n").unwrap();
    dir
}

fn start<'a>(cx: &'a mut TestAppContext, name: &str) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::search_view::init(cx);
        crate::quick_open::init(cx);
        crate::commands::init(cx);
        AppConfig::install(cx, None); // in memory: never the real settings file
    });
    let dir = workspace(name);
    let root = dir.clone();
    let (wb, cx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    cx.run_until_parked();
    (wb, cx, dir)
}

fn picker(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<Entity<QuickPick>> {
    wb.read_with(cx, |wb, _| wb.quick.picker().cloned())
}

fn query(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> String {
    picker(wb, cx).expect("the picker is open").read_with(cx, |p, cx| p.query(cx))
}

/// Labels of the rows; separators as "--label".
fn rows(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    let p = picker(wb, cx).expect("the picker is open");
    p.read_with(cx, |p, _| {
        p.list()
            .rows()
            .iter()
            .map(|r| match r {
                PickRow::Separator(s) => format!("--{s}"),
                PickRow::Item(i) => i.label.clone(),
            })
            .collect()
    })
}

/// The key binding shown on the row with this label, as one string ("⇧⌘P"), chords joined by a space.
fn keybinding_of_row(wb: &Entity<Workbench>, cx: &mut VisualTestContext, label: &str) -> Option<String> {
    let p = picker(wb, cx)?;
    p.read_with(cx, |p, _| {
        p.list().rows().iter().filter_map(PickRow::item).find(|i| i.label == label).map(|i| {
            i.keybinding.iter().map(|chord| chord.concat()).collect::<Vec<_>>().join(" ")
        })
    })
}

fn active_label(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<String> {
    picker(wb, cx)?.read_with(cx, |p, _| p.active_item().map(|i| i.label.clone()))
}

fn provider(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> String {
    wb.read_with(cx, |wb, _| wb.quick.provider.clone())
}

fn open_palette(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("cmd-shift-p");
    cx.run_until_parked();
}

fn type_text(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    cx.run_until_parked();
}

fn sidebar_visible(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> bool {
    wb.read_with(cx, |wb, _| wb.sidebar_visible)
}

#[gpui_kit::test]
fn shift_cmd_p_and_f1_open_the_palette_with_the_prefix(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "open");
    assert!(picker(&wb, cx).is_none());
    open_palette(cx);
    assert!(picker(&wb, cx).is_some());
    assert_eq!(query(&wb, cx), ">");
    assert_eq!(provider(&wb, cx), ">");
    assert!(rows(&wb, cx).contains(&"View: Toggle Primary Side Bar Visibility".to_string()));

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(picker(&wb, cx).is_none());
    cx.simulate_keystrokes("f1");
    cx.run_until_parked();
    assert_eq!(query(&wb, cx), ">", "F1 is the same command");
}

#[gpui_kit::test]
fn typing_toggle_side_then_enter_hides_the_side_bar(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "toggle");
    assert!(sidebar_visible(&wb, cx));
    open_palette(cx);
    type_text(cx, "toggle side");
    assert_eq!(query(&wb, cx), ">toggle side");
    let r = rows(&wb, cx);
    assert_eq!(r[0], "View: Toggle Primary Side Bar Visibility", "{r:?}");
    assert_eq!(active_label(&wb, cx).as_deref(), Some("View: Toggle Primary Side Bar Visibility"));
    assert_eq!(keybinding_of_row(&wb, cx, "View: Toggle Primary Side Bar Visibility").as_deref(), Some("⌘B"));

    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(picker(&wb, cx).is_none(), "accepting closes the palette");
    assert!(!sidebar_visible(&wb, cx), "the side bar is hidden");
}

#[gpui_kit::test]
fn cmd_b_toggles_the_side_bar(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "cmdb");
    assert!(sidebar_visible(&wb, cx));
    cx.simulate_keystrokes("cmd-b");
    cx.run_until_parked();
    assert!(!sidebar_visible(&wb, cx));
    cx.simulate_keystrokes("cmd-b");
    cx.run_until_parked();
    assert!(sidebar_visible(&wb, cx));
}

#[gpui_kit::test]
fn recently_used_commands_come_first(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "recent");
    open_palette(cx);
    let first_time = rows(&wb, cx);
    assert!(!first_time.iter().any(|r| r.starts_with("--")), "no history, no separators: {first_time:?}");
    assert_eq!(first_time[0], "Change Language Mode", "alphabetical: {first_time:?}");

    type_text(cx, "show explorer");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    open_palette(cx);
    type_text(cx, "toggle side");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    open_palette(cx);
    let r = rows(&wb, cx);
    assert_eq!(&r[..4], ["--recently used", "View: Toggle Primary Side Bar Visibility", "View: Show Explorer", "--other commands"], "{r:?}");
    assert_eq!(r[4], "Change Language Mode");

    // The history does not reorder an unrelated filter, and an unused match has no separator.
    type_text(cx, "settings");
    let r = rows(&wb, cx);
    assert_eq!(r[0], "Preferences: Open Settings (UI)");
    assert!(!r.iter().any(|x| x.starts_with("--")), "{r:?}");

    // Using a command again moves it to the front.
    cx.simulate_keystrokes("escape");
    open_palette(cx);
    type_text(cx, "show explorer");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    open_palette(cx);
    let r = rows(&wb, cx);
    assert_eq!(&r[..3], ["--recently used", "View: Show Explorer", "View: Toggle Primary Side Bar Visibility"], "{r:?}");
}

#[gpui_kit::test]
fn history_setting_zero_disables_recently_used_and_clear_command_history_empties_it(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "history");
    open_palette(cx);
    type_text(cx, "show explorer");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    open_palette(cx);
    assert_eq!(rows(&wb, cx)[0], "--recently used");

    type_text(cx, "clear command");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| cx.global::<Commands>().history.len()), 0);
    open_palette(cx);
    assert_ne!(rows(&wb, cx)[0], "--recently used");
    cx.simulate_keystrokes("escape");

    cx.update(|_, cx| AppConfig::set_value(cx, "workbench.commandPalette.history", Some(serde_json::json!(0))).unwrap());
    open_palette(cx);
    type_text(cx, "show explorer");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| cx.global::<Commands>().history.len()), 0, "history 0 records nothing");
}

#[gpui_kit::test]
fn backspace_over_the_prefix_switches_to_files_and_typing_it_switches_back(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "switch");
    open_palette(cx);
    assert_eq!(provider(&wb, cx), ">");
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert_eq!(query(&wb, cx), "");
    assert_eq!(provider(&wb, cx), "", "file mode");
    type_text(cx, "main");
    let r = rows(&wb, cx);
    assert!(r.contains(&"main.rs".to_string()) || r.iter().any(|x| x.contains("main")), "file results: {r:?}");

    // Typing '>' in front switches to commands.
    cx.simulate_keystrokes("home");
    type_text(cx, ">");
    assert_eq!(query(&wb, cx), ">main");
    assert_eq!(provider(&wb, cx), ">");
}

#[gpui_kit::test]
fn cmd_p_from_the_palette_and_shift_cmd_p_from_quick_open_switch_provider(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "keys");
    open_palette(cx);
    type_text(cx, "save");
    cx.simulate_keystrokes("cmd-p");
    cx.run_until_parked();
    assert_eq!(query(&wb, cx), "", "Cmd+P replaces the palette text with file mode");
    assert_eq!(provider(&wb, cx), "");
    cx.simulate_keystrokes("cmd-shift-p");
    cx.run_until_parked();
    assert_eq!(query(&wb, cx), ">");
    assert_eq!(provider(&wb, cx), ">");
    // The palette open: Shift+Cmd+P again keeps the text.
    type_text(cx, "toggle");
    cx.simulate_keystrokes("cmd-shift-p");
    cx.run_until_parked();
    assert_eq!(query(&wb, cx), ">toggle");
}

#[gpui_kit::test]
fn question_mark_lists_the_providers_and_enter_switches_to_one(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "help");
    cx.simulate_keystrokes("cmd-p");
    cx.run_until_parked();
    type_text(cx, "?");
    assert_eq!(provider(&wb, cx), "?");
    // `@` and `@:` are Go to Symbol in Editor (ADR-0087).
    assert_eq!(rows(&wb, cx), ["…", ":", ">", "@", "@:"]);
    let (desc, kb) = picker(&wb, cx).unwrap().read_with(cx, |p, _| {
        let items: Vec<_> = p.list().rows().iter().filter_map(PickRow::item).cloned().collect();
        (
            items.iter().map(|i| i.description.clone().unwrap()).collect::<Vec<_>>(),
            items.iter().map(|i| i.keybinding.iter().map(|c| c.concat()).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>(),
        )
    });
    assert_eq!(desc, ["Go to File", "Go to Line/Column", "Show and Run Commands", "Go to Symbol in Editor", "Go to Symbol in Editor (by kind)"]);
    assert_eq!(kb, ["⌘P", "⌃G", "⇧⌘P", "⇧⌘O", "⇧⌘O"]);

    cx.simulate_keystrokes("down down enter");
    cx.run_until_parked();
    assert_eq!(query(&wb, cx), ">", "the help entry switches provider");
    assert_eq!(provider(&wb, cx), ">");
}

#[gpui_kit::test]
fn go_to_line_with_colon_42_moves_the_caret_and_focuses_the_editor(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "goto");
    // No editor yet.
    cx.simulate_keystrokes("ctrl-g");
    cx.run_until_parked();
    assert_eq!(query(&wb, cx), ":");
    assert_eq!(rows(&wb, cx), ["Open a text editor first to go to a line or an offset."]);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    wb.update(cx, |wb, cx| {
        wb.open(&dir.join("src/long.txt"));
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-g");
    cx.run_until_parked();
    assert_eq!(provider(&wb, cx), ":");
    assert_eq!(rows(&wb, cx), ["Type a line number to go to (from 1 to 101)."], "100 lines and the final empty one");
    type_text(cx, "42");
    assert_eq!(query(&wb, cx), ":42");
    assert_eq!(rows(&wb, cx), ["Press 'Enter' to go to line 42 or enter colon : to add a column number."]);
    type_text(cx, ":3");
    assert_eq!(rows(&wb, cx), ["Press 'Enter' to go to line 42 at column 3."]);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(picker(&wb, cx).is_none());
    let pos = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selection().active);
    assert_eq!((pos.line, pos.column), (42, 3));

    // The editor has focus.
    cx.simulate_keystrokes("x");
    let line = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().buffer().line_content(42).to_string());
    assert_eq!(line, "lixne 42");
}

#[gpui_kit::test]
fn colon_in_quick_open_goes_to_the_line_of_the_active_editor(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "colon");
    wb.update(cx, |wb, cx| {
        wb.open(&dir.join("src/long.txt"));
        cx.notify();
    });
    cx.simulate_keystrokes("cmd-p");
    cx.run_until_parked();
    type_text(cx, ":42");
    assert_eq!(provider(&wb, cx), ":");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let line = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selection().active.line);
    assert_eq!(line, 42);
}

#[gpui_kit::test]
fn go_to_line_columns_are_visible_columns_and_offsets_work(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "gotocol");
    wb.update(cx, |wb, cx| {
        wb.open(&dir.join("src/tabs.txt"));
        cx.notify();
    });
    cx.simulate_keystrokes("ctrl-g");
    cx.run_until_parked();
    type_text(cx, "1:5");
    // Column 5 is the first character after the tab (tab size 4): real column 2.
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let pos = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selection().active);
    assert_eq!((pos.line, pos.column), (1, 2));

    cx.simulate_keystrokes("ctrl-g");
    cx.run_until_parked();
    type_text(cx, ":11"); // "::11": character 11 (1-based) is on the second line
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let pos = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selection().active);
    assert_eq!((pos.line, pos.column), (2, 1));
}

#[gpui_kit::test]
fn the_keybinding_shown_is_the_one_in_the_keymap(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "keybindings");
    wb.update(cx, |wb, cx| {
        wb.open(&dir.join("src/main.rs"));
        cx.notify();
    });
    open_palette(cx);
    let expected = [
        ("Show All Commands", "⇧⌘P"),
        ("Go to File...", "⌘P"),
        ("Go to Line/Column...", "⌃G"),
        ("View: Toggle Primary Side Bar Visibility", "⌘B"),
        ("View: Show Explorer", "⇧⌘E"),
        ("View: Show Search", "⇧⌘F"),
        ("Preferences: Open Settings (UI)", "⌘,"),
        ("File: Save", "⌘S"),
        ("View: Close Editor", "⌘W"),
        ("Select All", "⌘A"),
        ("Undo", "⌘Z"),
        ("Redo", "⇧⌘Z"),
        ("Copy", "⌘C"),
        ("Cut", "⌘X"),
        ("Paste", "⌘V"),
        ("Find", "⌘F"),
        ("Replace", "⌥⌘F"),
        ("Find Next", "⌘G"),
        ("Find Previous", "⇧⌘G"),
        ("Find with Selection", "⌘E"),
    ];
    for (label, kb) in expected {
        assert_eq!(keybinding_of_row(&wb, cx, label).as_deref(), Some(kb), "{label}");
    }
    assert_eq!(keybinding_of_row(&wb, cx, "Clear Command History").as_deref(), Some(""), "no binding, no label");

    // Every label equals what the keymap says, for every command of the palette.
    let mismatches: Vec<String> = cx.update(|_, app| {
        let commands = app.global::<Commands>();
        let ctx = crate::commands::context_keys(true, true);
        commands
            .registry
            .palette_commands(&ctx)
            .iter()
            .filter_map(|d| {
                let want = crate::commands::keybinding_of_command(&d.id, app);
                let got = picker_keybinding(&wb, app, &d.label());
                (want != got).then(|| format!("{}: {want:?} vs {got:?}", d.id))
            })
            .collect()
    });
    assert!(mismatches.is_empty(), "{mismatches:?}");
}

fn picker_keybinding(wb: &Entity<Workbench>, app: &gpui_kit::App, label: &str) -> Vec<Vec<String>> {
    let p = wb.read(app).quick.picker().cloned().expect("open");
    p.read(app).list().rows().iter().filter_map(PickRow::item).find(|i| i.label == label).map(|i| i.keybinding.clone()).unwrap_or_default()
}

#[gpui_kit::test]
fn editor_commands_run_on_the_active_editor_and_need_one(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "editor");
    open_palette(cx);
    let r = rows(&wb, cx);
    assert!(!r.contains(&"Find".to_string()) && !r.contains(&"File: Save".to_string()), "no editor: {r:?}");
    cx.simulate_keystrokes("escape");

    wb.update(cx, |wb, cx| {
        wb.open(&dir.join("src/main.rs"));
        cx.notify();
    });
    cx.run_until_parked();
    open_palette(cx);
    type_text(cx, "select all");
    assert_eq!(rows(&wb, cx)[0], "Select All");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let selected = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selected_text());
    assert_eq!(selected, "fn main() {}\n");

    // Find opens the widget in the editor.
    open_palette(cx);
    type_text(cx, "find");
    assert_eq!(rows(&wb, cx)[0], "Find");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.editor_view.find_is_open()), "the Find widget is open");
}

#[gpui_kit::test]
fn save_from_the_palette_writes_the_file(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "save");
    wb.update(cx, |wb, cx| {
        wb.open(&dir.join("src/main.rs"));
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("end");
    cx.simulate_input("// hi");
    open_palette(cx);
    type_text(cx, "file: save");
    assert_eq!(rows(&wb, cx)[0], "File: Save");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let text = fs::read_to_string(dir.join("src/main.rs")).unwrap();
    assert!(text.contains("// hi"), "{text:?}");
}

#[gpui_kit::test]
fn escape_closes_the_palette_and_returns_focus(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "escape");
    open_palette(cx);
    assert!(picker(&wb, cx).is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(picker(&wb, cx).is_none());
    let focused = cx.update(|window, app| wb.read(app).focus.as_ref().unwrap().is_focused(window));
    assert!(focused, "focus is back on the workbench");
}

#[gpui_kit::test]
fn preserve_input_restores_the_palette_text_after_the_prefix(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "preserve");
    cx.update(|_, cx| AppConfig::set_value(cx, "workbench.commandPalette.preserveInput", Some(serde_json::json!(true))).unwrap());
    open_palette(cx);
    type_text(cx, "settings");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    open_palette(cx);
    assert_eq!(query(&wb, cx), ">settings");
    // The restored text after the prefix is selected: typing replaces it, the prefix stays.
    type_text(cx, "fold");
    assert_eq!(query(&wb, cx), ">fold");
    assert_eq!(provider(&wb, cx), ">");
}

#[gpui_kit::test]
fn without_preserve_input_the_palette_starts_empty_with_the_prefix_kept_when_typing(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "nopreserve");
    open_palette(cx);
    type_text(cx, "settings");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    open_palette(cx);
    assert_eq!(query(&wb, cx), ">");
    type_text(cx, "sav");
    assert_eq!(query(&wb, cx), ">sav", "typing appends after the prefix");
}

#[gpui_kit::test]
fn palette_settings_are_read_from_the_configuration(cx: &mut TestAppContext) {
    cx.update(|cx| AppConfig::install(cx, None));
    cx.update(|cx| {
        let s = crate::quick_access::PaletteSettings::from_configuration(AppConfig::get(cx).configuration());
        assert_eq!((s.history, s.preserve_input), (50, false), "upstream defaults");
        AppConfig::set_value(cx, "workbench.commandPalette.history", Some(serde_json::json!(7))).unwrap();
        AppConfig::set_value(cx, "workbench.commandPalette.preserveInput", Some(serde_json::json!(true))).unwrap();
        let s = crate::quick_access::PaletteSettings::from_configuration(AppConfig::get(cx).configuration());
        assert_eq!((s.history, s.preserve_input), (7, true));
    });
}

#[gpui_kit::test]
fn show_explorer_and_search_commands_switch_the_view(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "views");
    cx.simulate_keystrokes("cmd-b"); // hide
    cx.run_until_parked();
    assert!(!sidebar_visible(&wb, cx));
    open_palette(cx);
    type_text(cx, "show explorer");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(sidebar_visible(&wb, cx), "Show Explorer opens the side bar");

    open_palette(cx);
    type_text(cx, "show search");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(sidebar_visible(&wb, cx));
    assert!(wb.read_with(cx, |wb, _| wb.search.is_some()), "the Search view was created");
}
