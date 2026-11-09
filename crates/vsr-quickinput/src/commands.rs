//! The picks of the Command Palette (`>`): filtering, sorting, "recently used".
//!
//! upstream: src/vs/platform/quickinput/browser/commandsQuickAccess.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/quickaccess/browser/commandsQuickAccess.ts @ 50e1350fc78d
//!
//! Same pipeline as `AbstractCommandsQuickAccessProvider._getPicks`: each command's label
//! (`Category: Title`) and its English alias are matched with `or(matchesBaseContiguousSubString,
//! matchesWords)`; commands whose id is exactly the filter match too; commands with the same label
//! get their id as description; the result is sorted most recently used first, then everything
//! else with the `Developer` category last and by name (`localeCompare`), with a "recently used"
//! and an "other commands" separator when the history contributes.
//!
//! Not ported: the tf-idf "similar commands" (filters of 3+ characters that match nothing),
//! `suggestedCommandIds` ("commonly used", experimental), the alias line (`showAlias` is for
//! localized builds), the AI related-information picks and per-item buttons.

use std::cmp::Ordering;
use std::collections::HashMap;

use vsr_commands::{CommandDescriptor, CommandsHistory};

use crate::filters::{matches_base_contiguous_sub_string, matches_words};
use crate::fuzzy::{Match, collate};
use crate::pick::{PickItem, PickRow};

/// `AbstractCommandsQuickAccessProvider.PREFIX`.
pub const PREFIX: &str = ">";

/// `Categories.Developer.value`: sorted after the other commands.
const DEVELOPER: &str = "Developer";

/// `WORD_FILTER = or(matchesBaseContiguousSubString, matchesWords)`.
pub fn word_filter(word: &str, target: &str) -> Option<Vec<Match>> {
    matches_base_contiguous_sub_string(word, target).or_else(|| matches_words(word, target, false))
}

struct Candidate<'a> {
    index: usize,
    command: &'a CommandDescriptor,
    label: String,
    highlights: Vec<Match>,
    description: Option<String>,
    used: Option<u64>,
}

/// The rows for `filter` (the text after `>`, trimmed). `commands` are the palette commands in
/// registration order; an item's `id` is its index there and its `value` the command id.
/// `keybinding_of` gives the chords of a command (empty when it has none).
pub fn command_picks(
    commands: &[&CommandDescriptor],
    filter: &str,
    history: &CommandsHistory,
    keybinding_of: &dyn Fn(&str) -> Vec<Vec<String>>,
) -> Vec<PickRow> {
    let mut found: Vec<Candidate> = Vec::new();
    for (index, command) in commands.iter().copied().enumerate() {
        let label = command.label();
        let label_hl = word_filter(filter, &label);
        let alias_hl = command.alias().and_then(|a| word_filter(filter, &a));
        if label_hl.is_none() && alias_hl.is_none() && filter != command.id {
            continue;
        }
        found.push(Candidate {
            index,
            command,
            label,
            highlights: label_hl.unwrap_or_default().into_iter().filter(|m| m.end > m.start).collect(),
            description: None,
            used: history.peek(&command.id),
        });
    }

    // Commands with the same label are told apart by their id.
    let mut by_label: HashMap<String, usize> = HashMap::new();
    for i in 0..found.len() {
        match by_label.get(&found[i].label).copied() {
            Some(first) => {
                found[i].description = Some(found[i].command.id.clone());
                found[first].description = Some(found[first].command.id.clone());
            }
            None => {
                by_label.insert(found[i].label.clone(), i);
            }
        }
    }

    found.sort_by(|a, b| match (a.used, b.used) {
        (Some(x), Some(y)) => y.cmp(&x), // more recently used first
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => {
            let dev = |c: &Candidate| c.command.category.as_deref() == Some(DEVELOPER);
            dev(a).cmp(&dev(b)).then_with(|| collate(&a.label, &b.label))
        }
    });

    let mut rows = Vec::with_capacity(found.len() + 2);
    let mut add_other_separator = false;
    for (i, c) in found.into_iter().enumerate() {
        let in_history = c.used.is_some();
        if i == 0 && in_history {
            rows.push(PickRow::Separator("recently used".into()));
            add_other_separator = true;
        }
        if add_other_separator && !in_history {
            rows.push(PickRow::Separator("other commands".into()));
            add_other_separator = false;
        }
        rows.push(PickRow::Item(PickItem {
            id: c.index,
            label: c.label,
            description: c.description,
            label_highlights: c.highlights,
            value: Some(c.command.id.clone()),
            keybinding: keybinding_of(&c.command.id),
            ..PickItem::default()
        }));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmds() -> Vec<CommandDescriptor> {
        vec![
            CommandDescriptor::new("workbench.action.toggleSidebarVisibility", "Toggle Primary Side Bar Visibility")
                .category("View")
                .f1(),
            CommandDescriptor::new("workbench.action.files.save", "Save").category("File").f1(),
            CommandDescriptor::new("workbench.action.openSettings", "Open Settings (UI)").category("Preferences").f1(),
            CommandDescriptor::new("editor.action.selectAll", "Select All").f1(),
            CommandDescriptor::new("workbench.action.toggleDevTools", "Toggle Developer Tools").category("Developer").f1(),
            CommandDescriptor::new("a.one", "Same").category("X").f1(),
            CommandDescriptor::new("b.two", "Same").category("X").f1(),
        ]
    }

    fn run(filter: &str, history: &CommandsHistory) -> Vec<String> {
        let all = cmds();
        let refs: Vec<&CommandDescriptor> = all.iter().collect();
        command_picks(&refs, filter, history, &|_| Vec::new())
            .into_iter()
            .map(|r| match r {
                PickRow::Separator(s) => format!("--{s}"),
                PickRow::Item(i) => i.label,
            })
            .collect()
    }

    #[test]
    fn empty_filter_lists_everything_by_name_with_developer_last() {
        let r = run("", &CommandsHistory::default());
        assert_eq!(r.len(), 7);
        assert_eq!(r[0], "File: Save");
        assert_eq!(r.last().unwrap(), "Developer: Toggle Developer Tools");
        assert!(r.iter().position(|l| l == "Select All") < r.iter().position(|l| l == "View: Toggle Primary Side Bar Visibility"));
    }

    #[test]
    fn filter_matches_contiguous_text_and_word_beginnings() {
        let h = CommandsHistory::default();
        assert_eq!(run("toggle side", &h)[0], "View: Toggle Primary Side Bar Visibility");
        assert_eq!(run("tpsb", &h), ["View: Toggle Primary Side Bar Visibility"], "word beginnings");
        assert_eq!(run("vtp", &h), ["View: Toggle Primary Side Bar Visibility"], "word beginnings");
        assert_eq!(run("File: Sa", &h), ["File: Save"]);
        assert_eq!(run("selectall", &h), ["Select All"]);
        assert!(run("xyz", &h).is_empty());
        assert_eq!(run("editor.action.selectAll", &h), ["Select All"], "an exact command id matches");
    }

    #[test]
    fn recently_used_first_with_separators() {
        let mut h = CommandsHistory::default();
        h.push("workbench.action.openSettings");
        h.push("editor.action.selectAll");
        let r = run("", &h);
        assert_eq!(&r[..4], ["--recently used", "Select All", "Preferences: Open Settings (UI)", "--other commands"]);
        assert_eq!(r[4], "File: Save");
    }

    #[test]
    fn no_separators_when_history_does_not_match_the_filter() {
        let mut h = CommandsHistory::default();
        h.push("editor.action.selectAll");
        let r = run("save", &h);
        assert_eq!(r, ["File: Save"]);
    }

    #[test]
    fn duplicate_labels_show_their_ids() {
        let all = cmds();
        let refs: Vec<&CommandDescriptor> = all.iter().collect();
        let rows = command_picks(&refs, "same", &CommandsHistory::default(), &|_| Vec::new());
        let descs: Vec<_> = rows.iter().filter_map(|r| r.item()).map(|i| i.description.clone().unwrap()).collect();
        assert_eq!(descs, ["a.one", "b.two"]);
    }

    #[test]
    fn items_carry_the_command_id_highlights_and_keybinding() {
        let all = cmds();
        let refs: Vec<&CommandDescriptor> = all.iter().collect();
        let kb = |id: &str| if id == "workbench.action.files.save" { vec![vec!["⌘".to_string(), "S".to_string()]] } else { vec![] };
        let rows = command_picks(&refs, "save", &CommandsHistory::default(), &kb);
        let item = rows[0].item().unwrap();
        assert_eq!(item.value.as_deref(), Some("workbench.action.files.save"));
        assert_eq!(item.keybinding, vec![vec!["⌘".to_string(), "S".to_string()]]);
        assert_eq!(item.label_highlights, vec![Match::new(6, 10)]);
        assert_eq!(item.id, 1, "index in the input list");
    }
}
