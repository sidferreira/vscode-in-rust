//! The Quick Access registry: which provider handles what is typed in the Quick Pick, and the
//! `?` help list.
//!
//! upstream: src/vs/platform/quickinput/common/quickAccess.ts @ 50e1350fc78d
//! upstream: src/vs/platform/quickinput/browser/helpQuickAccess.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/test/browser/quickAccess.test.ts @ 50e1350fc78d
//!
//! A provider is picked by the text typed: the one with the longest registered prefix that the
//! text starts with, else the default provider (empty prefix, Go to File). `>` is commands, `:`
//! is Go to Line/Column, `?` is help. The providers' behaviour lives in the app; this is the
//! routing and the descriptors (placeholder, help entries, `when`).
//!
//! Not ported: provider construction (`ctor`) and the `provide(picker, token)` life cycle, which
//! the app does with its own picker; `clear()`.

use std::rc::Rc;

use vsr_commands::{ContextKeys, When};

use crate::pick::PickItem;

/// `IQuickAccessProviderHelp`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpEntry {
    /// Shown instead of the provider's prefix.
    pub prefix: Option<String>,
    pub description: String,
    /// The command that brings the provider up (its key binding is shown).
    pub command_id: Option<String>,
}

/// `IQuickAccessProviderDescriptor`.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderDescriptor {
    pub prefix: String,
    pub placeholder: String,
    pub help_entries: Vec<HelpEntry>,
    pub when: Option<When>,
}

impl ProviderDescriptor {
    pub fn new(prefix: &str, placeholder: &str) -> Self {
        Self { prefix: prefix.into(), placeholder: placeholder.into(), help_entries: Vec::new(), when: None }
    }

    /// Add a help entry (`description`, and the command that opens the provider).
    pub fn help(mut self, description: &str, command_id: Option<&str>) -> Self {
        self.help_entries.push(HelpEntry { prefix: None, description: description.into(), command_id: command_id.map(String::from) });
        self
    }

    pub fn when(mut self, when: &str) -> Self {
        self.when = Some(When::parse(when).unwrap_or_else(|e| panic!("when {when:?}: {e}")));
        self
    }

    fn enabled(&self, ctx: &ContextKeys) -> bool {
        self.when.as_ref().is_none_or(|w| w.evaluate(ctx))
    }
}

/// A registered provider (compare with `Rc::ptr_eq`).
pub type Provider = Rc<ProviderDescriptor>;

#[derive(Default)]
pub struct QuickAccessRegistry {
    /// Longest prefix first.
    providers: Vec<Provider>,
    default_provider: Option<Provider>,
}

impl QuickAccessRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// `registerQuickAccessProvider`. Returns the registered provider (the handle for
    /// [`QuickAccessRegistry::unregister`]).
    pub fn register(&mut self, descriptor: ProviderDescriptor) -> Provider {
        let provider = Rc::new(descriptor);
        if provider.prefix.is_empty() {
            self.default_provider = Some(Rc::clone(&provider));
        } else {
            self.providers.push(Rc::clone(&provider));
        }
        // Longer prefixes first: `ext install` wins over `ext`.
        self.providers.sort_by_key(|a| std::cmp::Reverse(a.prefix.chars().count()));
        provider
    }

    pub fn unregister(&mut self, provider: &Provider) {
        if let Some(i) = self.providers.iter().position(|p| Rc::ptr_eq(p, provider)) {
            self.providers.remove(i);
        }
        if self.default_provider.as_ref().is_some_and(|d| Rc::ptr_eq(d, provider)) {
            self.default_provider = None;
        }
    }

    /// `getQuickAccessProviders`: the default provider then the others, those whose `when` holds.
    pub fn providers(&self, ctx: &ContextKeys) -> Vec<Provider> {
        self.default_provider.iter().chain(self.providers.iter()).filter(|p| p.enabled(ctx)).cloned().collect()
    }

    /// `getQuickAccessProvider(prefix)`: for the typed text, the provider with the longest
    /// matching prefix, else the default one.
    pub fn provider(&self, value: &str, ctx: &ContextKeys) -> Option<Provider> {
        let found = (!value.is_empty())
            .then(|| self.providers.iter().find(|p| value.starts_with(p.prefix.as_str()) && p.enabled(ctx)))
            .flatten();
        found.cloned().or_else(|| self.default_provider.clone())
    }
}

/// `HelpQuickAccessProvider.PREFIX`.
pub const HELP_PREFIX: &str = "?";

/// The items of the `?` list: every provider's help entries but the help provider's own, ordered
/// by prefix. An item's `value` is the prefix to switch to; the label is the prefix (`…` for the
/// default provider), the description what it does.
pub fn help_picks(registry: &QuickAccessRegistry, ctx: &ContextKeys, keybinding_of: &dyn Fn(&str) -> Vec<Vec<String>>) -> Vec<PickItem> {
    let mut providers = registry.providers(ctx);
    providers.sort_by(|a, b| crate::fuzzy::collate(&a.prefix, &b.prefix));
    let mut items = Vec::new();
    for provider in providers.iter().filter(|p| p.prefix != HELP_PREFIX) {
        for entry in &provider.help_entries {
            let prefix = entry.prefix.clone().unwrap_or_else(|| provider.prefix.clone());
            items.push(PickItem {
                id: items.len(),
                label: if prefix.is_empty() { "\u{2026}".to_string() } else { prefix.clone() },
                description: Some(entry.description.clone()),
                value: Some(prefix),
                keybinding: entry.command_id.as_deref().map(keybinding_of).unwrap_or_default(),
                ..PickItem::default()
            });
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(prefix: &str) -> ProviderDescriptor {
        ProviderDescriptor::new(prefix, "")
    }

    // upstream: QuickAccess > registry
    #[test]
    fn registry() {
        let mut registry = QuickAccessRegistry::new();
        let ctx = ContextKeys::default();

        assert!(registry.provider("test", &ctx).is_none());

        let default = registry.register(p(""));
        assert!(Rc::ptr_eq(&registry.provider("", &ctx).unwrap(), &default));
        assert!(Rc::ptr_eq(&registry.provider("test", &ctx).unwrap(), &default));

        let one = registry.register(p("test"));
        assert!(Rc::ptr_eq(&registry.provider("test", &ctx).unwrap(), &one));

        let providers = registry.providers(&ctx);
        assert!(providers.iter().any(|p| p.prefix == "test"));

        registry.unregister(&one);
        assert!(Rc::ptr_eq(&registry.provider("test", &ctx).unwrap(), &default));

        registry.unregister(&default);
        assert!(registry.provider("test", &ctx).is_none());
    }

    // upstream: QuickAccess > registry - when condition
    #[test]
    fn registry_when_condition() {
        let mut registry = QuickAccessRegistry::new();
        let mut ctx = ContextKeys::default();

        let with_when = registry.register(p("whentest").when("testQuickAccessContextKey"));
        assert!(registry.provider("whentest", &ctx).is_none());
        assert!(!registry.providers(&ctx).iter().any(|p| p.prefix == "whentest"));

        ctx.set_bool("testQuickAccessContextKey", true);
        assert!(Rc::ptr_eq(&registry.provider("whentest", &ctx).unwrap(), &with_when));
        assert!(registry.providers(&ctx).iter().any(|p| p.prefix == "whentest"));

        ctx.set_bool("testQuickAccessContextKey", false);
        assert!(registry.provider("whentest", &ctx).is_none());
    }

    #[test]
    fn longer_prefixes_win() {
        let mut registry = QuickAccessRegistry::new();
        let ctx = ContextKeys::default();
        let short = registry.register(p("ext"));
        let long = registry.register(p("ext install"));
        assert!(Rc::ptr_eq(&registry.provider("ext install foo", &ctx).unwrap(), &long));
        assert!(Rc::ptr_eq(&registry.provider("ext foo", &ctx).unwrap(), &short));
    }

    #[test]
    fn the_prefixes_of_the_workbench() {
        let mut registry = QuickAccessRegistry::new();
        let ctx = ContextKeys::default();
        let files = registry.register(p(""));
        let commands = registry.register(p(">"));
        let line = registry.register(p(":"));
        let help = registry.register(p("?"));
        let at = |s: &str| registry.provider(s, &ctx).unwrap();
        assert!(Rc::ptr_eq(&at(""), &files));
        assert!(Rc::ptr_eq(&at("main"), &files));
        assert!(Rc::ptr_eq(&at(">save"), &commands));
        assert!(Rc::ptr_eq(&at(":42"), &line));
        assert!(Rc::ptr_eq(&at("::10"), &line));
        assert!(Rc::ptr_eq(&at("?"), &help));
        assert!(Rc::ptr_eq(&at("a>b"), &files), "only a leading prefix counts");
    }

    #[test]
    fn help_lists_every_provider_but_help_itself() {
        let mut registry = QuickAccessRegistry::new();
        let ctx = ContextKeys::default();
        registry.register(p("").help("Go to File", Some("workbench.action.quickOpen")));
        registry.register(p(">").help("Show and Run Commands", Some("workbench.action.showCommands")));
        registry.register(p(":").help("Go to Line/Column", Some("workbench.action.gotoLine")));
        registry.register(p("?").help("Show all Quick Access Providers", None));
        let kb = |id: &str| if id == "workbench.action.gotoLine" { vec![vec!["⌃".to_string(), "G".to_string()]] } else { vec![] };
        let items = help_picks(&registry, &ctx, &kb);
        let shown: Vec<_> = items.iter().map(|i| (i.label.as_str(), i.description.as_deref().unwrap())).collect();
        assert_eq!(shown, [("…", "Go to File"), (":", "Go to Line/Column"), (">", "Show and Run Commands")]);
        assert_eq!(items[1].keybinding, vec![vec!["⌃".to_string(), "G".to_string()]]);
        assert_eq!(items[2].value.as_deref(), Some(">"));
    }
}
