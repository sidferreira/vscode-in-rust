//! upstream: src/vs/editor/common/languages.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/common/extHost.protocol.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/common/extHostTypeConverters.ts @ 50e1350fc78d
//!
//! What providers return, as Rust types: the `languages.ts` shapes (what the editor consumes)
//! converted from the DTOs the host sends (`extHostTypeConverters.ts` makes them; the main side's
//! `reviveXxx` in `mainThreadLanguageFeatures.ts` reads them). Every `from_json` is tolerant: a
//! field of the wrong type is dropped, an item that cannot be read at all is skipped, so one bad
//! provider result does not hide the rest.
//!
//! Positions and ranges are 1-based (`IPosition` / `IRange`), columns count UTF-16 units. Kinds
//! stay numeric as in `languages.ts` (see [`symbol_kind_name`], [`completion_item_kind_name`]).

use super::uri::Uri;
use serde_json::{Value, json};

fn u32_of(v: Option<&Value>) -> Option<u32> {
    v.and_then(Value::as_u64).map(|n| n as u32)
}

fn string_of(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).map(str::to_string)
}

fn array(v: Option<&Value>) -> impl Iterator<Item = &Value> {
    v.and_then(Value::as_array).into_iter().flatten()
}

/// `IPosition`: 1-based line and column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

impl Position {
    pub fn new(line: u32, column: u32) -> Position {
        Position { line, column }
    }

    pub fn to_json(self) -> Value {
        json!({ "lineNumber": self.line, "column": self.column })
    }

    pub fn from_json(v: &Value) -> Option<Position> {
        Some(Position::new(
            u32_of(v.get("lineNumber"))?,
            u32_of(v.get("column"))?,
        ))
    }
}

/// `IRange`: end exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

impl Range {
    pub fn new(start_line: u32, start_column: u32, end_line: u32, end_column: u32) -> Range {
        Range {
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }

    pub fn to_json(self) -> Value {
        json!({ "startLineNumber": self.start_line, "startColumn": self.start_column,
                "endLineNumber": self.end_line, "endColumn": self.end_column })
    }

    pub fn from_json(v: &Value) -> Option<Range> {
        Some(Range::new(
            u32_of(v.get("startLineNumber"))?,
            u32_of(v.get("startColumn"))?,
            u32_of(v.get("endLineNumber"))?,
            u32_of(v.get("endColumn"))?,
        ))
    }

    pub fn start(&self) -> Position {
        Position::new(self.start_line, self.start_column)
    }

    pub fn end(&self) -> Position {
        Position::new(self.end_line, self.end_column)
    }

    pub fn contains(&self, p: Position) -> bool {
        p >= self.start() && p <= self.end()
    }
}

/// `IMarkdownString` (the links collected in `uris` are not kept).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarkdownString {
    pub value: String,
    pub is_trusted: bool,
    pub support_html: bool,
    pub support_theme_icons: bool,
}

impl MarkdownString {
    pub fn from_json(v: &Value) -> Option<MarkdownString> {
        match v {
            Value::String(s) => Some(MarkdownString {
                value: s.clone(),
                ..Default::default()
            }),
            Value::Object(_) => Some(MarkdownString {
                value: string_of(v.get("value"))?,
                // `isTrusted` is a boolean or `{ enabledCommands }`: either way, trusted.
                is_trusted: v
                    .get("isTrusted")
                    .is_some_and(|t| t.as_bool().unwrap_or(t.is_object())),
                support_html: v
                    .get("supportHtml")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                support_theme_icons: v
                    .get("supportThemeIcons")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }),
            _ => None,
        }
    }
}

/// `string | IMarkdownString` (documentation, tooltips).
#[derive(Debug, Clone, PartialEq)]
pub enum Documentation {
    Text(String),
    Markdown(MarkdownString),
}

impl Documentation {
    pub fn from_json(v: &Value) -> Option<Documentation> {
        match v {
            Value::String(s) => Some(Documentation::Text(s.clone())),
            Value::Object(_) => MarkdownString::from_json(v).map(Documentation::Markdown),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Documentation::Text(s) => s,
            Documentation::Markdown(m) => &m.value,
        }
    }
}

/// `Command`; `ident` is the id the host gave a command it keeps for the main side to call back.
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub id: String,
    pub title: String,
    pub tooltip: Option<String>,
    pub arguments: Vec<Value>,
    pub ident: Option<String>,
}

impl Command {
    pub fn from_json(v: &Value) -> Option<Command> {
        Some(Command {
            id: string_of(v.get("id"))?,
            title: string_of(v.get("title")).unwrap_or_default(),
            tooltip: string_of(v.get("tooltip")),
            arguments: array(v.get("arguments")).cloned().collect(),
            ident: string_of(v.get("$ident")),
        })
    }
}

/// `TextEdit`: replace a range; `eol` is `EndOfLineSequence` (0 `LF`, 1 `CRLF`).
#[derive(Debug, Clone, PartialEq)]
pub struct TextEdit {
    pub range: Range,
    pub text: String,
    pub eol: Option<u32>,
}

impl TextEdit {
    pub fn from_json(v: &Value) -> Option<TextEdit> {
        Some(TextEdit {
            range: Range::from_json(v.get("range")?)?,
            text: string_of(v.get("text")).unwrap_or_default(),
            eol: u32_of(v.get("eol")),
        })
    }

    pub fn list_from_json(v: &Value) -> Vec<TextEdit> {
        array(Some(v)).filter_map(TextEdit::from_json).collect()
    }
}

/// `Location`.
#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    pub uri: Uri,
    pub range: Range,
}

impl Location {
    pub fn from_json(v: &Value) -> Option<Location> {
        Some(Location {
            uri: Uri::from_components(v.get("uri")?)?,
            range: Range::from_json(v.get("range")?)?,
        })
    }
}

/// `LocationLink` (definitions, declarations, implementations, type definitions).
#[derive(Debug, Clone, PartialEq)]
pub struct LocationLink {
    pub origin_selection_range: Option<Range>,
    pub uri: Uri,
    pub range: Range,
    pub target_selection_range: Option<Range>,
}

impl LocationLink {
    pub fn from_json(v: &Value) -> Option<LocationLink> {
        Some(LocationLink {
            origin_selection_range: v.get("originSelectionRange").and_then(Range::from_json),
            uri: Uri::from_components(v.get("uri")?)?,
            range: Range::from_json(v.get("range")?)?,
            target_selection_range: v.get("targetSelectionRange").and_then(Range::from_json),
        })
    }
}

/// `DocumentSymbol` (a tree; `kind` is `SymbolKind`).
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentSymbol {
    pub name: String,
    pub detail: String,
    pub kind: u32,
    pub tags: Vec<u32>,
    pub container_name: Option<String>,
    pub range: Range,
    pub selection_range: Range,
    pub children: Vec<DocumentSymbol>,
}

impl DocumentSymbol {
    pub fn from_json(v: &Value) -> Option<DocumentSymbol> {
        Some(DocumentSymbol {
            name: string_of(v.get("name"))?,
            detail: string_of(v.get("detail")).unwrap_or_default(),
            kind: u32_of(v.get("kind")).unwrap_or(0),
            tags: array(v.get("tags"))
                .filter_map(|t| t.as_u64().map(|n| n as u32))
                .collect(),
            container_name: string_of(v.get("containerName")),
            range: Range::from_json(v.get("range")?)?,
            selection_range: Range::from_json(v.get("selectionRange")?)?,
            children: array(v.get("children"))
                .filter_map(DocumentSymbol::from_json)
                .collect(),
        })
    }

    /// This symbol and its descendants, depth first (the outline's flat order).
    pub fn flatten(&self) -> Vec<&DocumentSymbol> {
        let mut out = vec![self];
        for c in &self.children {
            out.extend(c.flatten());
        }
        out
    }
}

/// `SymbolKind` as the outline names it (`Function`, `Class`, ...).
pub fn symbol_kind_name(kind: u32) -> &'static str {
    const NAMES: [&str; 26] = [
        "File",
        "Module",
        "Namespace",
        "Package",
        "Class",
        "Method",
        "Property",
        "Field",
        "Constructor",
        "Enum",
        "Interface",
        "Function",
        "Variable",
        "Constant",
        "String",
        "Number",
        "Boolean",
        "Array",
        "Object",
        "Key",
        "Null",
        "EnumMember",
        "Struct",
        "Event",
        "Operator",
        "TypeParameter",
    ];
    NAMES.get(kind as usize).copied().unwrap_or("Unknown")
}

/// `Hover`; `id` is the host's cache id (`HoverWithId`).
#[derive(Debug, Clone, PartialEq)]
pub struct Hover {
    pub contents: Vec<MarkdownString>,
    pub range: Option<Range>,
    pub id: Option<u64>,
    pub can_increase_verbosity: bool,
    pub can_decrease_verbosity: bool,
}

impl Hover {
    pub fn from_json(v: &Value) -> Option<Hover> {
        Some(Hover {
            contents: array(v.get("contents"))
                .filter_map(MarkdownString::from_json)
                .collect(),
            range: v.get("range").and_then(Range::from_json),
            id: v.get("id").and_then(Value::as_u64),
            can_increase_verbosity: v
                .get("canIncreaseVerbosity")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            can_decrease_verbosity: v
                .get("canDecreaseVerbosity")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
        .filter(|h| !h.contents.is_empty())
    }
}

/// `DocumentHighlight`; `kind` is `DocumentHighlightKind` (0 text, 1 read, 2 write).
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentHighlight {
    pub range: Range,
    pub kind: Option<u32>,
}

impl DocumentHighlight {
    pub fn from_json(v: &Value) -> Option<DocumentHighlight> {
        Some(DocumentHighlight {
            range: Range::from_json(v.get("range")?)?,
            kind: u32_of(v.get("kind")),
        })
    }
}

/// `FoldingRange`: 1-based lines; `kind` is the string id (`comment`, `imports`, `region`).
#[derive(Debug, Clone, PartialEq)]
pub struct FoldingRange {
    pub start: u32,
    pub end: u32,
    pub kind: Option<String>,
}

impl FoldingRange {
    pub fn from_json(v: &Value) -> Option<FoldingRange> {
        Some(FoldingRange {
            start: u32_of(v.get("start"))?,
            end: u32_of(v.get("end"))?,
            kind: v.get("kind").and_then(|k| {
                k.as_str()
                    .map(str::to_string)
                    .or_else(|| k.get("value").and_then(Value::as_str).map(str::to_string))
            }),
        })
    }
}

/// `CompletionItemInsertTextRule`.
pub const INSERT_TEXT_KEEP_WHITESPACE: u32 = 1;
pub const INSERT_TEXT_AS_SNIPPET: u32 = 4;

/// `CompletionItemKind` as the suggest widget names it.
pub fn completion_item_kind_name(kind: u32) -> &'static str {
    const NAMES: [&str; 29] = [
        "Method",
        "Function",
        "Constructor",
        "Field",
        "Variable",
        "Class",
        "Struct",
        "Interface",
        "Module",
        "Property",
        "Event",
        "Operator",
        "Unit",
        "Value",
        "Constant",
        "Enum",
        "EnumMember",
        "Keyword",
        "Text",
        "Color",
        "File",
        "Reference",
        "Customcolor",
        "Folder",
        "TypeParameter",
        "User",
        "Issue",
        "Tool",
        "Snippet",
    ];
    NAMES.get(kind as usize).copied().unwrap_or("Unknown")
}

/// Where a completion goes: the list's default ranges, one range, or insert and replace ranges.
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionRange {
    Single(Range),
    InsertReplace { insert: Range, replace: Range },
}

/// A `CompletionItem` (`ISuggestDataDto`, whose keys are single letters: `a` label ... `p`).
#[derive(Debug, Clone, PartialEq)]
pub struct CompletionItem {
    pub label: String,
    /// `CompletionItemLabel.detail` (after the label, no space) and `.description`.
    pub label_detail: Option<String>,
    pub label_description: Option<String>,
    pub kind: Option<u32>,
    pub tags: Vec<u32>,
    pub detail: Option<String>,
    pub documentation: Option<Documentation>,
    pub sort_text: Option<String>,
    pub filter_text: Option<String>,
    pub preselect: bool,
    /// What is inserted (the label when the provider gave none).
    pub insert_text: String,
    /// `CompletionItemInsertTextRule` flags.
    pub insert_text_rules: u32,
    pub range: CompletionRange,
    pub commit_characters: Vec<char>,
    pub additional_text_edits: Vec<TextEdit>,
    pub command: Option<Command>,
    /// `[list cache id, index]`: what `$resolveCompletionItem` takes.
    pub cache_id: Option<(u64, u64)>,
}

impl CompletionItem {
    pub fn is_snippet(&self) -> bool {
        self.insert_text_rules & INSERT_TEXT_AS_SNIPPET != 0
    }

    /// `ISuggestDataDto` with the list's default ranges for items that carry none.
    pub fn from_json(v: &Value, default: Option<&CompletionRange>) -> Option<CompletionItem> {
        let (label, label_detail, label_description) = match v.get("a")? {
            Value::String(s) => (s.clone(), None, None),
            o => (
                string_of(o.get("label"))?,
                string_of(o.get("detail")),
                string_of(o.get("description")),
            ),
        };
        let range = match v.get("j") {
            Some(r) if r.get("insert").is_some() => CompletionRange::InsertReplace {
                insert: Range::from_json(&r["insert"])?,
                replace: Range::from_json(&r["replace"])?,
            },
            Some(r) => CompletionRange::Single(Range::from_json(r)?),
            None => default?.clone(),
        };
        let cmd_id = string_of(v.get("o"));
        Some(CompletionItem {
            insert_text: string_of(v.get("h")).unwrap_or_else(|| label.clone()),
            label,
            label_detail,
            label_description,
            kind: u32_of(v.get("b")),
            tags: array(v.get("m"))
                .filter_map(|t| t.as_u64().map(|n| n as u32))
                .collect(),
            detail: string_of(v.get("c")),
            documentation: v.get("d").and_then(Documentation::from_json),
            sort_text: string_of(v.get("e")),
            filter_text: string_of(v.get("f")),
            preselect: v.get("g").and_then(Value::as_bool).unwrap_or(false),
            insert_text_rules: u32_of(v.get("i")).unwrap_or(0),
            range,
            commit_characters: string_of(v.get("k")).unwrap_or_default().chars().collect(),
            additional_text_edits: TextEdit::list_from_json(v.get("l").unwrap_or(&Value::Null)),
            command: cmd_id.map(|id| Command {
                title: String::new(),
                tooltip: None,
                arguments: array(v.get("p")).cloned().collect(),
                ident: string_of(v.get("n")),
                id,
            }),
            cache_id: v.get("x").and_then(|x| {
                let a = x.as_array()?;
                Some((a.first()?.as_u64()?, a.get(1)?.as_u64()?))
            }),
        })
    }

    /// The text the user types to match: `filterText`, else the label.
    pub fn filter_text(&self) -> &str {
        self.filter_text.as_deref().unwrap_or(&self.label)
    }

    /// The text items are sorted by: `sortText`, else the label.
    pub fn sort_text(&self) -> &str {
        self.sort_text.as_deref().unwrap_or(&self.label)
    }
}

/// `ISuggestResultDto`: what one provider offered (`a` default ranges, `b` items, `c` incomplete,
/// `d` duration, `x` the cache id to release).
#[derive(Debug, Clone, PartialEq)]
pub struct CompletionList {
    pub items: Vec<CompletionItem>,
    pub is_incomplete: bool,
    pub duration_ms: f64,
    pub cache_id: Option<u64>,
}

impl CompletionList {
    pub fn from_json(v: &Value) -> Option<CompletionList> {
        let default = v.get("a").and_then(|a| {
            Some(CompletionRange::InsertReplace {
                insert: Range::from_json(a.get("insert")?)?,
                replace: Range::from_json(a.get("replace")?)?,
            })
        });
        Some(CompletionList {
            items: array(v.get("b"))
                .filter_map(|i| CompletionItem::from_json(i, default.as_ref()))
                .collect(),
            is_incomplete: v.get("c").and_then(Value::as_bool).unwrap_or(false),
            duration_ms: v.get("d").and_then(Value::as_f64).unwrap_or(0.0),
            cache_id: v.get("x").and_then(Value::as_u64),
        })
    }
}

fn cache_id(v: &Value) -> Option<(u64, u64)> {
    let a = v.get("cacheId")?.as_array()?;
    Some((a.first()?.as_u64()?, a.get(1)?.as_u64()?))
}

/// `CodeLens`; `command` is absent until it is resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeLens {
    pub range: Range,
    pub command: Option<Command>,
    pub cache_id: Option<(u64, u64)>,
}

impl CodeLens {
    pub fn from_json(v: &Value) -> Option<CodeLens> {
        Some(CodeLens {
            range: Range::from_json(v.get("range")?)?,
            command: v.get("command").and_then(Command::from_json),
            cache_id: cache_id(v),
        })
    }
}

/// `ICodeLensListDto`.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeLensList {
    pub lenses: Vec<CodeLens>,
    pub cache_id: Option<u64>,
}

impl CodeLensList {
    pub fn from_json(v: &Value) -> Option<CodeLensList> {
        Some(CodeLensList {
            lenses: array(v.get("lenses"))
                .filter_map(CodeLens::from_json)
                .collect(),
            cache_id: v.get("cacheId").and_then(Value::as_u64),
        })
    }
}

/// One part of a structured inlay hint label.
#[derive(Debug, Clone, PartialEq)]
pub struct InlayHintLabelPart {
    pub label: String,
    pub tooltip: Option<Documentation>,
    pub command: Option<Command>,
    pub location: Option<Location>,
}

/// `InlayHint`; `kind` is 1 type, 2 parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct InlayHint {
    pub position: Position,
    pub label: Vec<InlayHintLabelPart>,
    pub kind: Option<u32>,
    pub text_edits: Vec<TextEdit>,
    pub tooltip: Option<Documentation>,
    pub padding_left: bool,
    pub padding_right: bool,
    pub cache_id: Option<(u64, u64)>,
}

impl InlayHint {
    pub fn from_json(v: &Value) -> Option<InlayHint> {
        let label = match v.get("label")? {
            Value::String(s) => vec![InlayHintLabelPart {
                label: s.clone(),
                tooltip: None,
                command: None,
                location: None,
            }],
            parts => array(Some(parts))
                .filter_map(|p| {
                    Some(InlayHintLabelPart {
                        label: string_of(p.get("label"))?,
                        tooltip: p.get("tooltip").and_then(Documentation::from_json),
                        command: p.get("command").and_then(Command::from_json),
                        location: p.get("location").and_then(Location::from_json),
                    })
                })
                .collect(),
        };
        Some(InlayHint {
            position: Position::from_json(v.get("position")?)?,
            label,
            kind: u32_of(v.get("kind")),
            text_edits: TextEdit::list_from_json(v.get("textEdits").unwrap_or(&Value::Null)),
            tooltip: v.get("tooltip").and_then(Documentation::from_json),
            padding_left: v
                .get("paddingLeft")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            padding_right: v
                .get("paddingRight")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            cache_id: cache_id(v),
        })
    }

    /// The label parts joined.
    pub fn text(&self) -> String {
        self.label.iter().map(|p| p.label.as_str()).collect()
    }
}

/// `IInlayHintsDto`.
#[derive(Debug, Clone, PartialEq)]
pub struct InlayHintList {
    pub hints: Vec<InlayHint>,
    pub cache_id: Option<u64>,
}

impl InlayHintList {
    pub fn from_json(v: &Value) -> Option<InlayHintList> {
        Some(InlayHintList {
            hints: array(v.get("hints"))
                .filter_map(InlayHint::from_json)
                .collect(),
            cache_id: v.get("cacheId").and_then(Value::as_u64),
        })
    }
}

/// `ParameterInformation.label`: the text, or the `[start, end)` offsets into the signature label.
#[derive(Debug, Clone, PartialEq)]
pub enum ParameterLabel {
    Text(String),
    Offsets(u32, u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParameterInformation {
    pub label: ParameterLabel,
    pub documentation: Option<Documentation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignatureInformation {
    pub label: String,
    pub documentation: Option<Documentation>,
    pub parameters: Vec<ParameterInformation>,
    pub active_parameter: Option<u32>,
}

/// `ISignatureHelpDto`.
#[derive(Debug, Clone, PartialEq)]
pub struct SignatureHelp {
    /// The host's cache id (`$releaseSignatureHelp`).
    pub id: Option<u64>,
    pub signatures: Vec<SignatureInformation>,
    pub active_signature: u32,
    pub active_parameter: u32,
}

impl SignatureHelp {
    pub fn from_json(v: &Value) -> Option<SignatureHelp> {
        Some(SignatureHelp {
            id: v.get("id").and_then(Value::as_u64),
            signatures: array(v.get("signatures"))
                .filter_map(|s| {
                    Some(SignatureInformation {
                        label: string_of(s.get("label"))?,
                        documentation: s.get("documentation").and_then(Documentation::from_json),
                        parameters: array(s.get("parameters"))
                            .filter_map(|p| {
                                Some(ParameterInformation {
                                    label: match p.get("label")? {
                                        Value::String(t) => ParameterLabel::Text(t.clone()),
                                        a => ParameterLabel::Offsets(
                                            u32_of(a.get(0))?,
                                            u32_of(a.get(1))?,
                                        ),
                                    },
                                    documentation: p
                                        .get("documentation")
                                        .and_then(Documentation::from_json),
                                })
                            })
                            .collect(),
                        active_parameter: u32_of(s.get("activeParameter")),
                    })
                })
                .collect(),
            active_signature: u32_of(v.get("activeSignature")).unwrap_or(0),
            active_parameter: u32_of(v.get("activeParameter")).unwrap_or(0),
        })
    }
}

/// `ILink` (a document link); `url` is a string or a URI.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentLink {
    pub range: Range,
    pub url: Option<String>,
    pub tooltip: Option<String>,
    pub cache_id: Option<(u64, u64)>,
}

impl DocumentLink {
    pub fn from_json(v: &Value) -> Option<DocumentLink> {
        Some(DocumentLink {
            range: Range::from_json(v.get("range")?)?,
            url: match v.get("url") {
                Some(Value::String(s)) => Some(s.clone()),
                Some(u @ Value::Object(_)) => Uri::from_components(u).map(|u| u.to_uri_string()),
                _ => None,
            },
            tooltip: string_of(v.get("tooltip")),
            cache_id: cache_id(v),
        })
    }
}

/// `ILinksListDto`.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentLinkList {
    pub links: Vec<DocumentLink>,
    pub cache_id: Option<u64>,
}

impl DocumentLinkList {
    pub fn from_json(v: &Value) -> Option<DocumentLinkList> {
        Some(DocumentLinkList {
            links: array(v.get("links"))
                .filter_map(DocumentLink::from_json)
                .collect(),
            cache_id: v.get("cacheId").and_then(Value::as_u64),
        })
    }
}

/// One entry of an `IWorkspaceEditDto`.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkspaceEditEntry {
    Text {
        resource: Uri,
        edit: TextEdit,
        version_id: Option<u64>,
    },
    /// Create (`old` none), delete (`new` none) or rename a file.
    File { old: Option<Uri>, new: Option<Uri> },
    /// Notebook cell edits and the like: kept raw.
    Other(Value),
}

/// `IWorkspaceEditDto`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkspaceEdit {
    pub edits: Vec<WorkspaceEditEntry>,
}

impl WorkspaceEdit {
    pub fn from_json(v: &Value) -> WorkspaceEdit {
        WorkspaceEdit {
            edits: array(v.get("edits"))
                .map(|e| {
                    if let (Some(resource), Some(te)) = (
                        e.get("resource").and_then(Uri::from_components),
                        e.get("textEdit").and_then(TextEdit::from_json),
                    ) {
                        WorkspaceEditEntry::Text {
                            resource,
                            edit: te,
                            version_id: e.get("versionId").and_then(Value::as_u64),
                        }
                    } else if e.get("oldResource").is_some() || e.get("newResource").is_some() {
                        WorkspaceEditEntry::File {
                            old: e.get("oldResource").and_then(Uri::from_components),
                            new: e.get("newResource").and_then(Uri::from_components),
                        }
                    } else {
                        WorkspaceEditEntry::Other(e.clone())
                    }
                })
                .collect(),
        }
    }
}

/// `ICodeActionDto`; `kind` is the `CodeActionKind` string (`quickfix`, `refactor.extract`, ...).
#[derive(Debug, Clone, PartialEq)]
pub struct CodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub is_preferred: bool,
    /// Why the action cannot be applied, when it cannot.
    pub disabled: Option<String>,
    pub edit: Option<WorkspaceEdit>,
    pub command: Option<Command>,
    pub diagnostics: Vec<Value>,
    pub cache_id: Option<(u64, u64)>,
}

impl CodeAction {
    pub fn from_json(v: &Value) -> Option<CodeAction> {
        Some(CodeAction {
            title: string_of(v.get("title"))?,
            kind: string_of(v.get("kind")),
            is_preferred: v
                .get("isPreferred")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            disabled: string_of(v.get("disabled")),
            edit: v.get("edit").map(WorkspaceEdit::from_json),
            command: v.get("command").and_then(Command::from_json),
            diagnostics: array(v.get("diagnostics")).cloned().collect(),
            cache_id: cache_id(v),
        })
    }
}

/// `ICodeActionListDto`.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeActionList {
    pub actions: Vec<CodeAction>,
    pub cache_id: Option<u64>,
}

impl CodeActionList {
    pub fn from_json(v: &Value) -> Option<CodeActionList> {
        Some(CodeActionList {
            actions: array(v.get("actions"))
                .filter_map(CodeAction::from_json)
                .collect(),
            cache_id: v.get("cacheId").and_then(Value::as_u64),
        })
    }
}

/// `RenameLocation`: the range that is renamed and its current text.
#[derive(Debug, Clone, PartialEq)]
pub struct RenameLocation {
    pub range: Range,
    pub text: String,
}

impl RenameLocation {
    pub fn from_json(v: &Value) -> Option<RenameLocation> {
        Some(RenameLocation {
            range: Range::from_json(v.get("range")?)?,
            text: string_of(v.get("text")).unwrap_or_default(),
        })
    }
}

/// `$provideRenameEdits`: the edit, or why the rename was rejected.
#[derive(Debug, Clone, PartialEq)]
pub struct RenameEdits {
    pub edit: WorkspaceEdit,
    pub reject_reason: Option<String>,
}

impl RenameEdits {
    pub fn from_json(v: &Value) -> Option<RenameEdits> {
        Some(RenameEdits {
            edit: WorkspaceEdit::from_json(v),
            reject_reason: string_of(v.get("rejectReason")),
        })
    }
}

/// `FormattingOptions`.
#[derive(Debug, Clone, PartialEq)]
pub struct FormattingOptions {
    pub tab_size: u32,
    pub insert_spaces: bool,
}

impl FormattingOptions {
    pub fn to_json(&self) -> Value {
        json!({ "tabSize": self.tab_size, "insertSpaces": self.insert_spaces })
    }
}
