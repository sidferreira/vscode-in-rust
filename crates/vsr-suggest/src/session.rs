//! The snippet session (ADR-0120): the tab stops of an inserted snippet, followed through the edits
//! made after it, so Tab can move to the next one and a typed placeholder rewrites its mirrors.
//!
//! Upstream source: src/vs/editor/contrib/snippet/browser/snippetSession.ts (`OneSnippet`: placeholder
//! groups by index, `next`/`prev`, `isAtLastPlaceholder`, `isSelectionWithinPlaceholders`,
//! `updateTabStopsFromDecorations`, the decoration stickiness) and snippetController2.ts (`_updateState`).
//!
//! The model holds UTF-16 offsets only. The editor reports each change as `(offset, removed, inserted)`
//! in the order it happened ([`SnippetSession::apply_change`]); nothing here reads the text except
//! through the `text_of` callback of [`SnippetSession::mirror_edits`].

use crate::snippet::Tabstop;

/// One tab stop of the session: every occurrence of an index, the first being the placeholder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub index: u32,
    /// `(start, end)` UTF-16 offsets in the document, in source order.
    pub ranges: Vec<(usize, usize)>,
}

impl Group {
    /// The placeholder's range (the first occurrence).
    pub fn primary(&self) -> (usize, usize) {
        self.ranges[0]
    }
}

/// The tab stops of one snippet insertion, and the current one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnippetSession {
    /// Ordered as upstream: `1..`, then `$0` (always last).
    groups: Vec<Group>,
    current: usize,
}

impl SnippetSession {
    /// The session for a snippet inserted at document offset `base`. `None` when the snippet has no
    /// placeholder besides `$0` (upstream starts no session for it).
    pub fn new(stops: &[Tabstop], base: usize) -> Option<Self> {
        if stops.iter().all(|t| t.index == 0) {
            return None;
        }
        let mut groups: Vec<Group> = stops
            .iter()
            .map(|t| Group {
                index: t.index,
                ranges: t.ranges.iter().map(|&(s, e)| (base + s, base + e)).collect(),
            })
            .collect();
        groups.sort_by_key(|g| if g.index == 0 { u32::MAX } else { g.index });
        Some(Self { groups, current: 0 })
    }

    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    pub fn current(&self) -> &Group {
        &self.groups[self.current]
    }

    /// Whether there is a stop after the current one (Tab does something).
    pub fn has_next(&self) -> bool {
        self.current + 1 < self.groups.len()
    }

    /// Whether there is a stop before the current one (Shift+Tab does something).
    pub fn has_prev(&self) -> bool {
        self.current > 0
    }

    /// Whether the current stop is the last one (`$0`, or the implicit end).
    pub fn at_last(&self) -> bool {
        !self.has_next()
    }

    /// Moves to the next stop. False when it is the last one already.
    pub fn next(&mut self) -> bool {
        if self.has_next() {
            self.current += 1;
            true
        } else {
            false
        }
    }

    /// Moves to the previous stop. False when it is the first one already.
    pub fn prev(&mut self) -> bool {
        if self.has_prev() {
            self.current -= 1;
            true
        } else {
            false
        }
    }

    /// The selection a stop shows: its placeholder, selected (a caret for an empty one).
    pub fn selection_of(&self, group: usize) -> (usize, usize) {
        self.groups[group].primary()
    }

    /// The range from the first stop's start to the last stop's end. Everything of the snippet lies
    /// in it.
    pub fn span(&self) -> (usize, usize) {
        let start = self.groups.iter().flat_map(|g| g.ranges.iter()).map(|r| r.0).min().unwrap_or(0);
        let end = self.groups.iter().flat_map(|g| g.ranges.iter()).map(|r| r.1).max().unwrap_or(start);
        (start, end)
    }

    /// Whether the selection `start..end` lies in the snippet's span (upstream
    /// `isSelectionWithinPlaceholders` is stricter; leaving the span ends the session here).
    pub fn contains(&self, start: usize, end: usize) -> bool {
        let (s, e) = self.span();
        s <= start && end <= e
    }

    /// The group and occurrence that contain `pos`, innermost (the shortest range) first. A
    /// position at a boundary counts for both neighbours; the shorter range wins.
    pub fn group_at(&self, pos: usize) -> Option<(usize, usize)> {
        let mut best: Option<((usize, usize), usize)> = None;
        for (gi, g) in self.groups.iter().enumerate() {
            for (ri, &(s, e)) in g.ranges.iter().enumerate() {
                if s <= pos && pos <= e && best.is_none_or(|(_, len)| e - s < len) {
                    best = Some(((gi, ri), e - s));
                }
            }
        }
        best.map(|(at, _)| at)
    }

    /// Follows one edit: the text in `[offset, offset + removed)` became `inserted` units long.
    /// Stops in the current group grow at their edges as they are typed into (their start stays, their
    /// end moves); other stops at the same boundary are pushed (their start moves right, their end
    /// stays), as upstream's decorations do.
    pub fn apply_change(&mut self, offset: usize, removed: usize, inserted: usize) {
        let current = self.current;
        for (gi, g) in self.groups.iter_mut().enumerate() {
            let side = match gi.cmp(&current) {
                std::cmp::Ordering::Less => Side::Before,
                std::cmp::Ordering::Equal => Side::Active,
                std::cmp::Ordering::Greater => Side::After,
            };
            for r in g.ranges.iter_mut() {
                r.0 = map_start(r.0, offset, removed, inserted, side);
                r.1 = map_end(r.1, offset, removed, inserted, side);
                if r.1 < r.0 {
                    r.1 = r.0;
                }
            }
        }
    }

    /// The edits that make the occurrences of the group `pos` is in equal to the text of the
    /// occurrence at `pos`: `(start, end, text)` in document offsets, one per mirror that differs.
    /// Empty when `pos` is in no stop. The caller applies them as one undo step and reports them
    /// through [`SnippetSession::apply_change`] like any other edit.
    pub fn mirror_edits(&self, pos: usize, text_of: impl Fn(usize, usize) -> String) -> Vec<(usize, usize, String)> {
        let Some((gi, ri)) = self.group_at(pos) else {
            return Vec::new();
        };
        let g = &self.groups[gi];
        let (s, e) = g.ranges[ri];
        let text = text_of(s, e);
        g.ranges
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != ri)
            .filter(|&(_, &(ms, me))| text_of(ms, me) != text)
            .map(|(_, &(ms, me))| (ms, me, text.clone()))
            .collect()
    }
}

/// Where a group stands against the current one (see [`SnippetSession::apply_change`]).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Before,
    Active,
    After,
}

/// A position moved through `[offset, offset + removed) -> inserted`, as a range start.
fn map_start(p: usize, offset: usize, removed: usize, inserted: usize, side: Side) -> usize {
    let delta = inserted as isize - removed as isize;
    if p < offset {
        p
    } else if p == offset {
        if removed == 0 && side == Side::After { offset + inserted } else { offset }
    } else if p < offset + removed {
        offset
    } else {
        (p as isize + delta) as usize
    }
}

/// A position moved through `[offset, offset + removed) -> inserted`, as a range end.
fn map_end(p: usize, offset: usize, removed: usize, inserted: usize, side: Side) -> usize {
    let delta = inserted as isize - removed as isize;
    if p < offset {
        p
    } else if p == offset {
        if removed == 0 && side != Side::Before { offset + inserted } else { offset }
    } else if p < offset + removed {
        offset + inserted
    } else {
        (p as isize + delta) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snippet::expand;

    /// The session of `snippet` inserted at 0, over the text it expands to.
    fn session(snippet: &str) -> (SnippetSession, String) {
        let e = expand(snippet);
        (SnippetSession::new(&e.stops, 0).expect("a placeholder"), e.text)
    }

    #[test]
    fn stops_run_in_index_order_with_zero_last() {
        let (mut s, _) = session("$2 ${1:a} $0 $3");
        let order: Vec<u32> = s.groups().iter().map(|g| g.index).collect();
        assert_eq!(order, vec![1, 2, 3, 0]);
        assert!(s.has_next() && !s.has_prev());
        assert!(s.next() && s.next() && s.next());
        assert!(s.at_last());
        assert!(!s.next(), "no stop after $0");
        assert!(s.prev());
        assert_eq!(s.current().index, 3);
    }

    #[test]
    fn no_session_without_a_placeholder() {
        let e = expand("foo($0)");
        assert!(SnippetSession::new(&e.stops, 0).is_none());
        let e = expand("plain");
        assert!(SnippetSession::new(&e.stops, 0).is_none());
    }

    #[test]
    fn the_selection_is_the_placeholder_and_the_base_shifts_it() {
        let e = expand("let ${1:name} = $0");
        let s = SnippetSession::new(&e.stops, 10).expect("session");
        assert_eq!(s.selection_of(0), (14, 18));
        assert_eq!(s.span(), (14, 21));
        assert!(s.contains(14, 18) && !s.contains(9, 10));
    }

    #[test]
    fn typing_in_the_active_stop_grows_it_and_moves_the_later_ones() {
        // `${1:a}$2` at 0: "a", stop 2 at 1.
        let (mut s, text) = session("${1:a}$2");
        assert_eq!(text, "a");
        // Type "bc" at the end of the placeholder (offset 1).
        s.apply_change(1, 0, 2);
        assert_eq!(s.groups()[0].ranges, vec![(0, 3)], "the active stop grows at its end");
        assert_eq!(s.groups()[1].ranges, vec![(3, 3)], "the stop after is pushed");
    }

    #[test]
    fn a_stop_at_the_same_boundary_is_pushed_only_when_after() {
        // `$1${2:x}` : stop 1 empty at 0, stop 2 "x" at 0..1.
        let (mut s, _) = session("$1${2:x}");
        s.next(); // current = stop 2 (index 2)
        // Typing at offset 0: the stop before (empty, at 0) stays before the text, the active one grows.
        s.apply_change(0, 0, 1);
        assert_eq!(s.groups()[0].ranges, vec![(0, 0)], "a stop before the active one stays at the edge");
        assert_eq!(s.groups()[1].ranges, vec![(0, 2)], "the active stop grows at its start");
        assert_eq!(s.groups()[2].ranges, vec![(2, 2)], "the final stop after it is pushed past the text");
    }

    #[test]
    fn a_replacement_inside_a_stop_snaps_its_edges() {
        let (mut s, _) = session("${1:abcdef}$0");
        // Replace "cd" (offset 2..4) with "XYZ".
        s.apply_change(2, 2, 3);
        assert_eq!(s.groups()[0].ranges, vec![(0, 7)]);
        assert_eq!(s.groups()[1].ranges, vec![(7, 7)]);
    }

    #[test]
    fn editing_a_placeholder_rewrites_its_mirrors() {
        // `${1:x} = $1` -> "x = x" with stop 1 at (0,1) and (4,5).
        let (s, _) = session("${1:x} = $1");
        // The document now reads "hello = x": the first occurrence was typed into.
        let doc = "hello = x";
        let mut s = s;
        s.apply_change(0, 1, 5);
        let edits = s.mirror_edits(2, |a, b| doc.chars().skip(a).take(b - a).collect());
        assert_eq!(edits, vec![(8, 9, "hello".to_string())]);
    }

    #[test]
    fn no_mirror_edits_outside_the_stops() {
        let (s, _) = session("${1:x} y");
        assert!(s.mirror_edits(5, |_, _| String::new()).is_empty());
    }

    #[test]
    fn leaving_the_span_is_not_contained() {
        let (s, _) = session("a${1:b}c");
        // "abc": the placeholder b at 1..2, the implicit $0 at 3; the span is 1..3.
        assert_eq!(s.span(), (1, 3));
        assert!(s.contains(1, 2) && s.contains(3, 3));
        assert!(!s.contains(0, 1) && !s.contains(2, 4));
    }
}
