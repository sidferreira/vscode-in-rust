//! Go to Symbol in Editor (`@`, and `@:` grouped by kind), ADR-0087: the symbols of the active
//! file in a Quick Pick, filtered by the text after the prefix; Enter selects the symbol.
//!
//! Upstream source: src/vs/editor/contrib/quickAccess/browser/gotoSymbolQuickAccess.ts
//!
//! Differences: no category headers for `@:` (the rows are sorted by kind and show the kind),
//! and no kind filter (upstream's Go to Symbol does not apply `show<Kind>` either).

use vsr_exthost::language_features::dto::symbol_kind_name;
use vsr_exthost::language_features::outline::natural_cmp;
use vsr_quickinput::pick::{PickItem, PickRow, filter_items};
use vsr_exthost::language_features::dto::Range as SymbolRange;

use crate::Workbench;
use crate::config::settings::OpenSource;

impl Workbench {
    /// The rows for the typed `query` (`@name` or `@:name`).
    pub(crate) fn go_to_symbol_rows(&self, query: &str, grouped: bool) -> Vec<PickRow> {
        let typed = query.trim_start();
        let text = typed
            .strip_prefix("@:")
            .or_else(|| typed.strip_prefix('@'))
            .unwrap_or(typed)
            .trim();
        let Some(path) = self.active_path() else {
            return Vec::new();
        };
        let Some(model) = self.outline.model(&path) else {
            return Vec::new();
        };
        let mut symbols = model.list_of_symbols();
        if grouped {
            symbols.sort_by(|a, b| {
                a.kind
                    .cmp(&b.kind)
                    .then_with(|| natural_cmp(&a.name, &b.name))
            });
        }
        let items: Vec<PickItem> = symbols
            .iter()
            .enumerate()
            .map(|(id, s)| {
                let mut item = PickItem::new(id, s.name.clone());
                item.description = Some(match &s.container_name {
                    Some(c) if !grouped => c.clone(),
                    _ => symbol_kind_name(s.kind).to_string(),
                });
                let r = s.selection_range;
                item.value = Some(format!(
                    "{},{},{},{}",
                    r.start_line, r.start_column, r.end_line, r.end_column
                ));
                item
            })
            .collect();
        if text.is_empty() {
            items.into_iter().map(PickRow::Item).collect()
        } else {
            filter_items(&items, text)
        }
    }

    /// Enter on a symbol: the active file, its selection range selected (ADR-0087).
    pub(crate) fn accept_go_to_symbol(
        &mut self,
        value: Option<String>,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let Some(path) = self.active_path() else {
            return;
        };
        let Some(v) = value else { return };
        let nums: Vec<usize> = v
            .split(',')
            .filter_map(|n| n.parse::<usize>().ok())
            .collect();
        let [sl, sc, el, ec] = nums[..] else { return };
        self.hide_quick_open(false, window, cx);
        // The picker's values are the symbol's 1-based range.
        let range = SymbolRange::new(sl as u32, sc as u32, el as u32, ec as u32);
        self.select_symbol(&path, range, OpenSource::QuickOpen, window, cx);
    }
}
