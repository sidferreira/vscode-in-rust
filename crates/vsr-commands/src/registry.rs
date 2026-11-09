//! `CommandsRegistry` and the command palette metadata.
//!
//! upstream: src/vs/platform/commands/common/commands.ts @ 50e1350fc78d
//! upstream: src/vs/platform/actions/common/actions.ts @ 50e1350fc78d
//! upstream: src/vs/platform/commands/test/common/commands.test.ts @ 50e1350fc78d
//!
//! A command is an id plus a handler. The palette metadata (`Action2`: `title`, `category`, `f1`,
//! `precondition`, `metadata.description`) lives in the same entry, so there is one place where
//! menus, key bindings, the Command Palette and the extension API (`commands.executeCommand`)
//! look a command up. The handler type `H` is the caller's: the app stores gpui actions, the
//! extension host will store a proxy that calls back into the extension process.
//!
//! Like upstream, registering an id that already exists overrides it; disposing the newer
//! registration brings the older one back.
//!
//! Differences: titles are plain strings (English), with an optional `original_title` for the
//! alias match of localized builds (`ILocalizedString.original`); upstream's
//! `MenuRegistry.addCommand` entries without `f1` are simply `f1: false`; there is no
//! `onDidRegisterCommand` event yet.

use std::collections::HashMap;

use crate::when::{ContextKeys, When};

/// What a command is, for menus and the Command Palette (`ICommandAction` + `IAction2Options`).
#[derive(Clone, Debug, PartialEq)]
pub struct CommandDescriptor {
    pub id: String,
    /// `title.value`.
    pub title: String,
    /// `title.original`, when the title is localized and differs from the English one.
    pub original_title: Option<String>,
    /// `category.value`, e.g. `View`, `File`, `Preferences`.
    pub category: Option<String>,
    /// `f1: true`: listed in the Command Palette.
    pub f1: bool,
    /// The command is enabled (and listed) only when this holds.
    pub precondition: Option<When>,
    /// `metadata.description`.
    pub description: Option<String>,
}

impl CommandDescriptor {
    /// A command with just an id and a title.
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            original_title: None,
            category: None,
            f1: false,
            precondition: None,
            description: None,
        }
    }

    pub fn category(mut self, category: &str) -> Self {
        self.category = Some(category.to_string());
        self
    }

    /// List the command in the Command Palette.
    pub fn f1(mut self) -> Self {
        self.f1 = true;
        self
    }

    /// Panics on an invalid expression: preconditions are written in code.
    pub fn precondition(mut self, when: &str) -> Self {
        self.precondition = Some(When::parse(when).unwrap_or_else(|e| panic!("precondition {when:?}: {e}")));
        self
    }

    pub fn description(mut self, text: &str) -> Self {
        self.description = Some(text.to_string());
        self
    }

    /// The palette label: `Category: Title`.
    pub fn label(&self) -> String {
        match &self.category {
            Some(c) => format!("{c}: {}", self.title),
            None => self.title.clone(),
        }
    }

    /// The English label (`commandAlias`), `None` when the title has no separate original.
    pub fn alias(&self) -> Option<String> {
        let title = self.original_title.as_deref()?;
        Some(match &self.category {
            Some(c) => format!("{c}: {title}"),
            None => title.to_string(),
        })
    }

    /// Whether the precondition holds (a missing one always does).
    pub fn enabled(&self, ctx: &ContextKeys) -> bool {
        self.precondition.as_ref().is_none_or(|w| w.evaluate(ctx))
    }
}

/// A handle to undo one registration (`IDisposable` of `registerCommand`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Registration(u64);

struct Entry<H> {
    token: Registration,
    descriptor: CommandDescriptor,
    handler: H,
}

pub struct CommandsRegistry<H> {
    next: u64,
    /// Registration order of the ids that have at least one entry.
    order: Vec<String>,
    /// Per id, the registrations; the last one is in effect.
    entries: HashMap<String, Vec<Entry<H>>>,
}

impl<H> Default for CommandsRegistry<H> {
    fn default() -> Self {
        Self { next: 1, order: Vec::new(), entries: HashMap::new() }
    }
}

impl<H> CommandsRegistry<H> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a command. If the id exists the new registration overrides it.
    pub fn register(&mut self, descriptor: CommandDescriptor, handler: H) -> Registration {
        let token = Registration(self.next);
        self.next += 1;
        let id = descriptor.id.clone();
        let stack = self.entries.entry(id.clone()).or_default();
        if stack.is_empty() {
            self.order.push(id);
        }
        stack.push(Entry { token, descriptor, handler });
        token
    }

    /// Undo a registration; an older one for the same id comes back into effect.
    pub fn unregister(&mut self, token: Registration) {
        let Some(id) = self.entries.iter().find(|(_, s)| s.iter().any(|e| e.token == token)).map(|(id, _)| id.clone()) else {
            return;
        };
        let stack = self.entries.get_mut(&id).expect("found above");
        stack.retain(|e| e.token != token);
        if stack.is_empty() {
            self.entries.remove(&id);
            self.order.retain(|o| *o != id);
        }
    }

    fn current(&self, id: &str) -> Option<&Entry<H>> {
        self.entries.get(id)?.last()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.current(id).is_some()
    }

    pub fn descriptor(&self, id: &str) -> Option<&CommandDescriptor> {
        self.current(id).map(|e| &e.descriptor)
    }

    pub fn handler(&self, id: &str) -> Option<&H> {
        self.current(id).map(|e| &e.handler)
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Every command in effect, in registration order (`getCommands`).
    pub fn commands(&self) -> impl Iterator<Item = (&CommandDescriptor, &H)> {
        self.order.iter().filter_map(|id| self.current(id)).map(|e| (&e.descriptor, &e.handler))
    }

    /// The commands the Command Palette lists: `f1` and enabled in `ctx`
    /// (`MenuId.CommandPalette` filtered by `precondition`).
    pub fn palette_commands(&self, ctx: &ContextKeys) -> Vec<&CommandDescriptor> {
        self.commands().map(|(d, _)| d).filter(|d| d.f1 && d.enabled(ctx)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(id: &str) -> CommandDescriptor {
        CommandDescriptor::new(id, id)
    }

    // upstream: Command Tests > register/dispose
    #[test]
    fn register_and_dispose() {
        let mut r = CommandsRegistry::new();
        let reg = r.register(d("foo"), 1);
        assert_eq!(r.handler("foo"), Some(&1));
        r.unregister(reg);
        assert!(r.handler("foo").is_none());
        assert!(r.is_empty());
    }

    // upstream: Command Tests > register/register/dispose
    #[test]
    fn override_and_dispose_in_either_order() {
        let mut r = CommandsRegistry::new();
        let reg1 = r.register(d("foo"), 1);
        let reg2 = r.register(d("foo"), 2);
        assert_eq!(r.handler("foo"), Some(&2));
        r.unregister(reg2);
        assert_eq!(r.handler("foo"), Some(&1));
        r.unregister(reg1);
        assert!(r.handler("foo").is_none());

        let reg1 = r.register(d("foo"), 1);
        let reg2 = r.register(d("foo"), 2);
        assert_eq!(r.handler("foo"), Some(&2));
        r.unregister(reg1);
        assert_eq!(r.handler("foo"), Some(&2), "disposing the overridden one changes nothing");
        r.unregister(reg2);
        assert!(r.handler("foo").is_none());
    }

    #[test]
    fn commands_keep_registration_order_and_one_entry_per_id() {
        let mut r = CommandsRegistry::new();
        r.register(d("b"), ());
        r.register(d("a"), ());
        r.register(d("b"), ());
        let ids: Vec<_> = r.commands().map(|(d, _)| d.id.as_str()).collect();
        assert_eq!(ids, ["b", "a"]);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn labels_have_the_category_and_the_english_alias() {
        let mut c = CommandDescriptor::new("x", "Ouvrir").category("Fichier");
        assert_eq!(c.label(), "Fichier: Ouvrir");
        assert_eq!(c.alias(), None);
        c.original_title = Some("Open".into());
        assert_eq!(c.alias().as_deref(), Some("Fichier: Open"));
        assert_eq!(CommandDescriptor::new("y", "Go").label(), "Go");
    }

    #[test]
    fn the_palette_lists_f1_commands_whose_precondition_holds() {
        let mut r = CommandsRegistry::new();
        r.register(d("hidden"), ());
        r.register(d("always").f1(), ());
        r.register(d("needsEditor").f1().precondition("hasActiveEditor"), ());
        let mut ctx = ContextKeys::default();
        let ids = |ctx: &ContextKeys| r.palette_commands(ctx).iter().map(|d| d.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&ctx), ["always"]);
        ctx.set_bool("hasActiveEditor", true);
        assert_eq!(ids(&ctx), ["always", "needsEditor"]);
    }
}
