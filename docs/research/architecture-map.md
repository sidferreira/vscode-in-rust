# VS Code architecture map → Rust workspace

Upstream: `../vscode-original` @ `50e1350fc78df67b11049b903d96b530f28d26a5` (v1.147.0). All paths below are relative to that clone unless prefixed `vscode-rust/`. Counts were taken with grep on that commit.

---

## 1. Layers in `src/vs`

| Layer | Owns | Notes |
|---|---|---|
| `src/vs/base/` | Foundation, no services: strings, charCode, uri, path, glob, json, event/Emitter, lifecycle/Disposable, async, arrays, color, keyCodes/keybindings, marshalling. `base/browser/` = DOM helpers + UI widgets (list, tree, splitview, grid, scrollbar). `base/parts/ipc/` = IPC channels + wire protocol. `base/node/` = fs/process helpers. | Imports only `vs/base/**`. |
| `src/vs/platform/` | ~112 service areas, each `platform/<svc>/{common,browser,node,electron-main}`: `instantiation` (DI), `files`, `configuration`, `keybinding`, `theme` (color registry), `storage`, `log`, `commands`, `contextkey`, `environment`, `workspace`, `undoRedo`, `registry`, `windows`, `ipc`, `extensionManagement`, ... | Interfaces + default impls. Imports `base` and other `platform/*`. |
| `src/vs/editor/` | The Monaco editor. `editor/common/` = model (text buffer, TextModel, decorations, tokens), core types (Position/Range/Selection), cursor, viewModel, viewLayout, languages registries, config/editorOptions. `editor/browser/` = DOM view (`view/`, `viewParts/`, widgets). `editor/contrib/` = features (find, folding, suggest, hover...). `editor/standalone/` = Monaco build. | Imports `base`, `platform`, `editor`. Never workbench. |
| `src/vs/workbench/` | The application shell. `browser/` = layout + parts (titlebar, activitybar, sidebar, editor, panel, statusbar); `services/` = workbench-level service impls (themes, keybinding, configuration, textfile, editor, extensions...); `contrib/` = features (files explorer, search, scm, debug, terminal, chat...); `api/` = extension host API (`common/extHost*.ts`, `browser/mainThread*.ts`, `node/extensionHostProcess.ts`). | Imports everything below it. `contrib/*` must go through services, not each other. |
| `src/vs/code/` | Electron app entry: `electron-main/` (main process: `main.ts`, `app.ts`, windows, lifecycle), `electron-browser/workbench/` (renderer bootstrap), `electron-utility/`, `node/` (CLI). | Equivalent of our `vsr-app` binary. |
| `src/vs/server/` | `server/node/` remote server (`server.main.ts`, `extensionHostConnection.ts`). | Useful reference for spawning an ext host without Electron. |
| `src/vs/sessions/` | New in this version: Agent sessions window, a workbench sibling that may import `vs/workbench` but not vice versa. | Out of scope. |
| `cli/` | Already Rust (crate `code-cli`, lib `cli`, tokio/serde/rmp-serde): tunnels, self-update, `code` CLI. | Reusable reference for RPC/async idioms; not part of the editor. |

### Layering rules (enforced by `eslint.config.js`)

Environment folders (`local/code-layering`, `eslint.config.js:117`), each may import only:

```
common            -> (nothing but common)
browser           -> common                  (DOM, no Node)
node              -> common                  (Node, no DOM)
electron-browser  -> common, browser         (renderer w/ sandboxed preload)
electron-utility  -> common, node            (Electron utility process)
electron-main     -> common, node, electron-utility
```

Module layering (`local/code-import-patterns`, `eslint.config.js:1786+`): `base` → `base` only; `platform/*` → `base` + `platform/*`; `editor` → `base` + `platform/*` + `editor`; `workbench` → all of the above + `editor/contrib/*` + `workbench/services/*`. `common/` is the portable, testable core and is what we port first. `browser/` maps to our native UI crates; `node/`/`electron-main/` map to native platform services.

### Dependency injection / service model

Files: `src/vs/platform/instantiation/common/{instantiation.ts,instantiationService.ts,serviceCollection.ts,descriptors.ts,extensions.ts,graph.ts}`.

- `createDecorator<T>('id')` returns a `ServiceIdentifier<T>` that is both a unique key and a TypeScript parameter decorator. Writing `constructor(@IFileService private files: IFileService)` records `{id, index}` on the class (`$di$dependencies`).
- `registerSingleton(IFoo, FooImpl, InstantiationType.Delayed|Eager)` pushes into a module-global registry; at startup the registry is copied into a `ServiceCollection`.
- `IInstantiationService.createInstance(Ctor, ...staticArgs)` resolves decorated ctor params (static args come first, services last), creating services lazily and detecting cycles (`graph.ts`). `invokeFunction(fn)` hands a `ServicesAccessor`. `createChild(services)` creates scoped containers (for example per editor).
- Rule: services are declared in constructors only; never pulled from the instantiation service later.

**Rust mapping.** No decorators: each service is a trait (`trait FileService: Send + Sync`), the identifier is the trait's `TypeId` (or a `ServiceId<T>` const with a name for debugging), and the container is a typed map `ServiceRegistry { HashMap<TypeId, Arc<dyn Any>> }` with `get::<dyn FileService>()`. Constructors take explicit `Arc<dyn Trait>` args; a `Services` struct per layer (built in `vsr-app`) replaces `createInstance`. Child scopes = a registry with a parent pointer. Keep lazy/eager as an enum only if startup cost needs it. Avoid a global registry: wire it in `vsr-app`.

---

## 2. Proposed crate layout

Dependency direction (no cycles, mirrors upstream layering):

```
vsr-base  <-  vsr-text  <-  vsr-editor  <-  vsr-workbench  <-  vsr-app
    ^                           ^                ^               |
    +------  vsr-platform  -----+----------------+               |
    +------  vsr-rpc  <-  vsr-exthost  <-------------------------+
```

Arrows point from dependency to dependent: `vsr-platform` depends only on `vsr-base` (and may use `vsr-text` value types such as `Position`/`Range`); `vsr-editor-view`, `vsr-workbench` and `vsr-exthost` depend on `vsr-platform`. `vsr-text` and `vsr-editor` (model side) must not depend on `vsr-platform` or any UI crate; that keeps the editor core reusable behind a webview workbench later. `vsr-workbench` never depends on `vsr-rpc`/`vsr-exthost` directly; it talks to extension features through `vsr-platform` traits (commands, language features), which `vsr-exthost` implements.

| Crate | Purpose | Mirrors (upstream paths) |
|---|---|---|
| `vsr-base` | Pure utilities, no services. | `src/vs/base/common/{strings,charCode,uri,path,glob,json,event,lifecycle,color,keyCodes,keybindings,arrays,marshallingIds}.ts` |
| `vsr-text` (exists) | Text buffer + core types. Only these upstream deps: | `src/vs/editor/common/model/pieceTreeTextBuffer/{pieceTreeBase,pieceTreeTextBuffer,pieceTreeTextBufferBuilder,rbTreeBase}.ts`, `src/vs/editor/common/core/{position,range,selection,textChange}.ts`, `src/vs/editor/common/core/misc/eolCounter.ts`, `src/vs/editor/common/model/prefixSumComputer.ts`, the buffer slice of `src/vs/editor/common/model.ts` (`ITextBuffer`, `IReadonlyTextBuffer`, `EndOfLinePreference`, `DefaultEndOfLine`, `EndOfLineSequence`), later `src/vs/editor/common/model/textModelSearch.ts` |
| `vsr-editor` | TextModel (edits, undo stack, decorations, tokens), cursor, view model, view layout. Headless, UI-toolkit independent. | `src/vs/editor/common/model/{textModel,editStack,intervalTree,indentationGuesser,mirrorTextModel}.ts`, `src/vs/editor/common/cursor/`, `src/vs/editor/common/viewModel/`, `src/vs/editor/common/viewLayout/`, `src/vs/editor/common/config/{editorOptions,fontInfo}.ts`, `src/vs/editor/common/languages/`, `src/vs/editor/common/tokens/` |
| `vsr-editor-view` (optional split) | Native rendering of the editor widget (glyph layout, gutter, cursor, scrollbars). Kept separate so a webview could replace it. | `src/vs/editor/browser/view/`, `src/vs/editor/browser/viewParts/` |
| `vsr-platform` | Service traits + native impls: files, configuration, keybindings, themes/color registry, storage, log, commands, context keys, environment, workspace, undo/redo. Service container lives here. | `src/vs/platform/instantiation/common/`, `src/vs/platform/files/{common,node}/`, `src/vs/platform/configuration/common/`, `src/vs/platform/keybinding/common/`, `src/vs/platform/theme/common/{colorRegistry.ts,colors/,tokenClassificationRegistry.ts}`, `src/vs/platform/storage/common/`, `src/vs/platform/log/common/`, `src/vs/platform/commands/common/`, `src/vs/platform/contextkey/common/`, `src/vs/platform/environment/common/`, `src/vs/platform/workspace/common/`, `src/vs/platform/undoRedo/common/`, `src/vs/platform/registry/common/`; workbench-level impls from `src/vs/workbench/services/{themes,keybinding,configuration,textfile}/` (e.g. `themes/common/colorThemeData.ts`) |
| `vsr-workbench` | Native UI shell: layout grid, parts (title, activity bar, sidebar, editor groups/tabs, panel, status bar), explorer. | `src/vs/workbench/browser/layout.ts`, `src/vs/workbench/browser/workbench.ts`, `src/vs/workbench/browser/parts/`, `src/vs/workbench/common/editor/`, `src/vs/workbench/services/editor/`, `src/vs/workbench/contrib/files/` |
| `vsr-rpc` | Ext-host wire protocol only (framing, message codec, proxy-id table). No UI deps. | `src/vs/base/parts/ipc/common/ipc.net.ts` (`PersistentProtocol`), `src/vs/workbench/services/extensions/common/{rpcProtocol,proxyIdentifier,extensionHostProtocol,extensionHostEnv}.ts` |
| `vsr-exthost` (future) | Spawns Node ext host, does the handshake, implements `MainThread*` shapes on top of `vsr-platform`/`vsr-editor`. | `src/vs/workbench/api/common/extHost.protocol.ts`, `src/vs/workbench/api/browser/mainThread*.ts` (100 files), `src/vs/workbench/services/extensions/electron-browser/localProcessExtensionHost.ts`, `src/vs/server/node/extensionHostConnection.ts` |
| `vsr-app` | Binary: wires services, opens window, CLI args. | `src/vs/code/electron-main/{main,app}.ts`, `src/vs/code/electron-browser/workbench/`, `src/vs/platform/windows/electron-main/` |

---

## 3. First TDD target: the editor text buffer

`vscode-rust/DECISIONS.md` ADR-0005 already fixes the model: VS Code semantics, UTF-16 offsets, `Vec<Line>` storage first, piece tree later. The tests below are chosen so they pass against either storage.

### Upstream test files (all under `src/vs/editor/test/common/model/`)

| File | Tests / suites | What it covers |
|---|---|---|
| `pieceTreeTextBuffer/pieceTreeTextBuffer.test.ts` | **87 / 11** | Primary target. Suites: inserts and deletes; prefix sum for line feed; offset 2 position; get text in range; CRLF; centralized lineStarts with CRLF; random is unsupervised; buffer api; search offset cache; snapshot; chunk based search. |
| `linesTextBuffer/linesTextBufferBuilder.test.ts` | 12 / 1 | Despite its name it tests `createTextBufferFactory` (builder): EOL detection, BOM, RTL/ASCII flags. |
| `linesTextBuffer/linesTextBuffer.test.ts` | 29 / 2 | Tests `PieceTreeTextBuffer._getInverseEdits` (undo edit computation). Port with `applyEdits`. |
| `textModel.test.ts` | 40 / 4 | `TextModelData.fromString` suite (5 tests) uses `createTextBuffer` directly, so it is portable now. The rest needs `TextModel` + DI (`getValueLengthInRange`, `validatePosition`, indentation guessing, `createSnapshot`). |
| `model.line.test.ts` | 6 / 2 | Line tokens; later. |
| `model.test.ts` | 32 / 3 | TextModel edit events; later (`vsr-editor`). |
| `editableTextModel.test.ts` (+ `editableTextModelAuto.test.ts`, `editableTextModelTestUtils.ts`) | 62 / 4 | `applyEdits` correctness; second milestone. |

Test helpers to port alongside (`pieceTreeTextBuffer.test.ts:1-212`): `createTextBuffer(chunks, normalizeEOL = true)` (builder → `factory.create(DefaultEndOfLine.LF)`), `testLinesContent(str, pt)` (checks line count, raw content, each line, each line range against `splitLines`), `testLineStarts(str, pt)` (offset ↔ position round-trip at every line start), and `assertTreeInvariants` (red-black tree checks). Add a `#[cfg(test)] fn assert_invariants(&self)` that is a no-op for `Vec<Line>` and real once the piece tree lands. Skip `getNearestChunk` and the `random is unsupervised` suite until the piece tree exists (they depend on piece boundaries or are fuzzers; the fuzzer is worth porting as a `proptest` later).

### Ten first Rust tests (quoted from the TS)

Unless noted, from `pieceTreeTextBuffer.test.ts`, `pieceTable = createTextBuffer([...]).getPieceTree()`.

1. **basic insert/delete** (l.218)
   ```ts
   createTextBuffer(['This is a document with some text.'])
   pieceTable.insert(34, 'This is some more text to insert at offset 34.');
   // getLinesRawContent() === 'This is a document with some text.This is some more text to insert at offset 34.'
   pieceTable.delete(42, 5);
   // getLinesRawContent() === 'This is a document with some text.This is more text to insert at offset 34.'
   ```
2. **more inserts** (l.238): from `['']`: `insert(0,'AAA')`→`'AAA'`; `insert(0,'BBB')`→`'BBBAAA'`; `insert(6,'CCC')`→`'BBBAAACCC'`; `insert(5,'DDD')`→`'BBBAADDDACCC'`.
3. **more deletes** (l.253): from `['012345678']`: `delete(8,1)`→`'01234567'`; `delete(0,1)`→`'1234567'`; `delete(5,1)`→`'123457'`; `delete(5,1)`→`'12345'`; `delete(0,5)`→`''`.
4. **prefix sum basic** (l.581): `['1\n2\n3\n4']` → `getLineCount() === 4`; `getPositionAt(0..6)` = `(1,1) (1,2) (2,1) (2,2) (3,1) (3,2) (4,1)`; `getOffsetAt(1,1)=0, (1,2)=1, (2,1)=2, (2,2)=3, (3,1)=4, (3,2)=5, (4,1)=6`.
5. **prefix sum append** (l.605): `['a\nb\nc\nde']`, `insert(8, 'fh\ni\njk')` → `getLineCount() === 6`, `getPositionAt(9)` = `(4,4)`, `getOffsetAt(1,1) === 0`.
6. **prefix sum delete** (l.642): `['a\nb\nc\ndefh\ni\njk']`, `delete(7, 2)` → raw `'a\nb\nc\ndh\ni\njk'`, 6 lines; `getPositionAt(6)=(4,1), (7)=(4,2), (8)=(4,3), (9)=(5,1), (11)=(6,1), (12)=(6,2), (13)=(6,3)`; `getOffsetAt(4,1)=6, (4,2)=7, (4,3)=8, (5,1)=9, (6,1)=11, (6,2)=12, (6,3)=13`. Note `getPositionAt(8)` lands on the `\n` and reports column 3 of line 4.
7. **getContentInRange** (l.862): `['a\nb\nc\nde']`, `insert(8,'fh\ni\njk')`, `delete(7,2)` → `getValueInRange(Range(1,1,1,3)) === 'a\n'`, `(2,1,2,3)='b\n'`, `(3,1,3,3)='c\n'`, `(4,1,4,4)='dh\n'`, `(5,1,5,3)='i\n'`, `(6,1,6,3)='jk'`. A column one past the line end includes the EOL.
8. **get line content basic** (l.986): `['1\n2\n3\n4']` → `getLineRawContent(1..4)` = `'1\n','2\n','3\n','4'` (raw keeps EOL; `getLineContent` strips it). Pair with **get line content** (l.975): `['1']` → `'1'`; `insert(1,'2')` → `'12'`.
9. **delete CR in CRLF 1 / 2** (l.1057, l.1068): `createTextBuffer([''], false)` (no normalization), `insert(0, 'a\r\nb')`, then `delete(0, 2)` → `getLineCount() === 2` (leaves `'\nb'`); or `delete(2, 2)` → `getLineCount() === 2` (leaves `'a\r'`). Lone `\r` and lone `\n` are each a line break.
10. **getLineCharCode - issue #47733** (l.1804): chunks `['', 'LINE1\n', 'line2']` → `getLineCharCode(1,0..5)` = `'L','I','N','E','1','\n'`, `getLineCharCode(2,0..4)` = `'l','i','n','e','2'`. Checks empty chunks are skipped and index is 0-based.

Builder / factory tests (from `linesTextBuffer/linesTextBufferBuilder.test.ts`, `testTextBufferFactory(text, eol, mightContainNonBasicASCII, mightContainRTL)`, created with `DefaultEndOfLine.LF`):

11. `('', '\n', false, false)`; `('Hello world\nHow are you?', '\n', false, false)`.
12. **carriage return detection**: `'Hello world\r\nHow are you?\nIs everything good today?\nDo you enjoy the weather?'` → `'\n'` (1 CRLF vs 2 LF); with 2 CRLF + 1 LF → `'\r\n'`; with 3 CRLF → `'\r\n'`.
13. **BOM**: `UTF8_BOM_CHARACTER + 'Hello world!'` → `'\n'`, false, false (BOM stripped, kept in `getBOM()`); **ASCII handling 2**: `'Hello world!!\nHow do you do?Züricha📚📚b'` → `'\n'`, true, false.

And `textModel.test.ts` suite `TextModelData.fromString` (l.110) **multiline text**:

```ts
'Hello,\r\ndear friend\nHow\rare\r\nyou?'
// => { EOL: '\r\n', lines: ['Hello,', 'dear friend', 'How', 'are', 'you?'], containsRTL: false, isBasicASCII: true }
```
(3 of 4 breaks are CR-based → CRLF; lone `\r` splits `How`/`are`; mixed EOLs normalized.)

Later, once `TextModel` exists (`textModel.test.ts`): `getValueLengthInRange` (l.189, l.220) and `validatePosition` (l.779: on `'line one\nline two'`, `(0,0)→(1,1)`, `(1,30)→(1,9)`, `(3,1)→(2,9)`, `(30,30)→(2,9)`, NaN/float handling). Range/position clamping lives in `TextModel`, not the buffer.

### APIs to match

From `IReadonlyTextBuffer` (`src/vs/editor/common/model.ts:1530`) and `PieceTreeBase` (`pieceTreeBase.ts`):

- `getLineCount()`: number of line breaks + 1. Empty buffer has 1 line; trailing `\n` gives an empty last line.
- `getLineContent(n)`: line without its EOL. Uses `_EOLLength` when normalized, else strips `/(\r\n|\r|\n)$/`. `getLineRawContent(n)` keeps the EOL. `getLinesContent()` returns all lines.
- `getLineLength(n)`, `getLineMaxColumn(n) = length + 1`, `getLineCharCode(n, index0)`, `getCharCode(offset)`, `getLength()`.
- `getOffsetAt(line, column)` = start offset of line + `column - 1` (no clamping in the buffer).
- `getPositionAt(offset)`: `Math.floor`, clamps to `>= 0`, empty tree → `(1,1)`; offsets past the end land on the last line.
- `getValueInRange(range, EndOfLinePreference)`, `getValueLengthInRange`, `getCharacterCountInRange` (counts surrogate pairs as 1), `getRangeAt(offset, len)`.
- `insert(offset, text)`, `delete(offset, count)` (PieceTreeBase); `applyEdits(...)` + `setEOL(eol)` (ITextBuffer).
- `getEOL()`, `getBOM()`, `mightContainRTL()`, `mightContainNonBasicASCII()`, `mightContainUnusualLineTerminators()` (U+2028 LS / U+2029 PS, `UNUSUAL_LINE_TERMINATORS` in `src/vs/base/common/strings.ts:699`; used to offer removing them).

### Semantics we must match

- **Positions:** line and column are 1-based; offsets are 0-based. **Columns and offsets count UTF-16 code units** (JS string indices). Evidence: `getCharacterCountInRange` surrogate loop in `pieceTreeTextBuffer.ts`, `validatePosition around high-low surrogate pairs` tests in `textModel.test.ts`, and the extension API (`vscode.Position.character`) is UTF-16. ADR-0005 already chose UTF-16 at the API. Converting at the ext-host boundary instead would be possible but every protocol range would need conversion; keep UTF-16 in `vsr-text`.
- **Line breaks:** `\r\n`, lone `\r`, and lone `\n` are each one break (`createLineStartsFast`, `pieceTreeBase.ts:37`). U+2028/U+2029 are not line breaks for the buffer; they only set `mightContainUnusualLineTerminators`.
- **EOL choice** (`PieceTreeTextBufferFactory._getEOL`, `pieceTreeTextBufferBuilder.ts`): no breaks → `DefaultEndOfLine` (LF by default); else `'\r\n'` iff `cr + crlf > (cr + lf + crlf) / 2`; otherwise `'\n'`. Lone `\r` counts toward CR.
- **Normalization:** `finish(normalizeEOL = true)` (the default, and what `TextModel` uses) rewrites every `\r\n|\r|\n` to the chosen EOL when the text is mixed. The CRLF suites pass `false` to test raw behaviour. Raw `PieceTreeBase.insert` does not normalize; `PieceTreeTextBuffer.applyEdits` rewrites edit text whose EOL (via `countEOL`) differs from the buffer EOL (`pieceTreeTextBuffer.ts:276-286`), so a model never becomes mixed again.
- **Builder chunking:** empty chunks are skipped; a chunk ending in `\r` or a high surrogate holds that char back for the next chunk (so `\r` + `\n` across chunks becomes one CRLF); BOM (U+FEFF) is stripped from the first chunk and reported by `getBOM()`; `isBasicASCII` = every non-break char is Tab or in 32..126; RTL detected via `strings.containsRTL`. Piece tree splits large chunks at `AverageBufferSize = 65535`, never between `\r\n` or a surrogate pair.
- **`EndOfLinePreference`**: `TextDefined=0`, `LF=1`, `CRLF=2` (for reads); `DefaultEndOfLine`: `LF=1`, `CRLF=2` (for creation); `EndOfLineSequence`: `LF=0`, `CRLF=1` (for `setEOL`/ext-host). Three different enums with different numbering.

---

## 4. Extension host compatibility notes

**Shape of the API.** `src/vs/workbench/api/common/extHost.protocol.ts` (4,406 lines) declares **89 `MainThread*Shape`** interfaces (services the ext host calls on the main side, about **544 `$`-methods**) and **83 `ExtHost*Shape`** interfaces (calls into the ext host, about **487 methods**). `MainContext` lists 89 proxy identifiers and `ExtHostContext` 82 (171 total). Upstream implementations: 100 files `src/vs/workbench/api/browser/mainThread*.ts`, 116 files `src/vs/workbench/api/common/extHost*.ts`. Public API surface: `src/vscode-dts/vscode.d.ts` (21.5k lines).

**RPC layer** (`src/vs/workbench/services/extensions/common/rpcProtocol.ts`, 968 lines). Proxies turn `proxy.$method(args)` into a request message; replies resolve promises; cancellation tokens become a separate `Cancel` message; a 3 s "unresponsive" timer drives the ext host responsiveness UI.
- Inner message (`MessageBuffer`, big-endian): `type: u8`, `req: u32`, then body. Requests: `rpcId: u8`, `method` as short string (u8 length + UTF-8), args as long string (u32 length + JSON) or a mixed array (`ArgType` String=1, VSBuffer=2, SerializedObjectWithBuffers=3, Undefined=4).
- `MessageType`: RequestJSONArgs=1, RequestJSONArgsWithCancellation=2, RequestMixedArgs=3, RequestMixedArgsWithCancellation=4, Acknowledged=5, Cancel=6, ReplyOKEmpty=7, ReplyOKVSBuffer=8, ReplyOKJSON=9, ReplyOKJSONWithBuffers=10, ReplyErrError=11, ReplyErrEmpty=12.
- **`rpcId` is not a name.** It is `ProxyIdentifier.nid`, a global counter assigned in declaration order of `createProxyIdentifier(...)` calls (`proxyIdentifier.ts:33-52`); only `extHost.protocol.ts` calls it. A Rust main side must reproduce exactly that order: generate the id table from `extHost.protocol.ts` (MainContext first, then ExtHostContext) with a script, and re-check on every upstream sync.
- JSON args carry revived types tagged with `$mid` (`src/vs/base/common/marshallingIds.ts`, enum `MarshalledId`: Uri=1, Regexp=2, Date, plus SCM/comment/notebook/chat contexts). Ranges and positions travel as plain `{startLineNumber, startColumn, ...}` DTOs (types in `extHost.protocol.ts` / `src/vs/workbench/api/common/extHostTypeConverters.ts`). URIs may be transformed (`uriTransformer`) for remote setups.

**Outer framing** (`src/vs/base/parts/ipc/common/ipc.net.ts`, `PersistentProtocol`): 13-byte header `type: u8, id: u32, ack: u32, dataLength: u32` (big-endian), types None=0, Regular=1, Control=2, Ack=3, Disconnect=5, ReplayRequest=6, Pause=7, Resume=8, KeepAlive=9. It acks, replays on reconnect, and keeps alive every 5 s (timeout 20 s). RPC messages travel as `Regular` payloads.

**Handshake** (`extensionHostProtocol.ts:123`, `localProcessExtensionHost.ts:470-520`): ext host sends 1-byte `Ready` (2) → main sends JSON `IExtensionHostInitData` (version, commit, environment, workspace, extension descriptions, log levels, ...) → ext host replies 1-byte `Initialized` (1). `Terminate` (3) shuts it down. 60 s timeout each.

**Launch.** Entry module is `vs/workbench/api/node/extensionHostProcess` loaded via `bootstrap-fork` with `VSCODE_ESM_ENTRYPOINT=vs/workbench/api/node/extensionHostProcess`. Transport chosen by env (`extensionHostEnv.ts`): desktop uses Electron `utilityProcess.fork` + MessagePort (`VSCODE_WILL_SEND_MESSAGE_PORT`); the remote server uses `cp.fork(bootstrap-fork)` + socket handoff (`VSCODE_EXTHOST_WILL_SEND_SOCKET`, `src/vs/server/node/extensionHostConnection.ts:290`); the simplest for us is the named pipe: set `VSCODE_EXTHOST_IPC_HOOK=<unix socket path>`, the ext host connects with `net.createConnection` and speaks `PersistentProtocol`, then sends `Resume` (`extensionHostProcess.ts:276-285`).

**What a Rust main side needs to host unmodified extensions:**
1. Ship upstream's compiled ext host JS (`out/vs/workbench/api/node/extensionHostProcess.js`, `bootstrap-fork.js` and their deps; MIT) and run it with a system or bundled `node`. No Electron needed. Our clone has no `out/` (never built); the JS can come from a built VS Code, e.g. `/Applications/Visual Studio Code.app/Contents/Resources/app/out/` (present on this machine), or from a one-off upstream build on another disk.
2. `vsr-rpc`: socket server, `PersistentProtocol` framing, `MessageBuffer` codec, `$mid` JSON marshalling, generated proxy-id table, handshake.
3. Build `IExtensionHostInitData`: scan extension `package.json` manifests (`src/vs/platform/extensionManagement/`, `src/vs/workbench/services/extensions/common/extensionsRegistry.ts`) and activation events.
4. Implement a minimal set of `MainThread*` shapes first: ExtensionService, Commands, Configuration, DocumentsAndEditors / Documents / TextEditors (mirror models as UTF-16 line/column change events, cf. `src/vs/editor/common/model/mirrorTextModel.ts`), Workspace, FileSystem, MessageService, StatusBar, OutputService, Diagnostics, LanguageFeatures, Logger, Storage, Telemetry (stub). Call the matching `ExtHost*` methods (`$initializeConfiguration`, `$acceptDocumentsAndEditorsDelta`, `$acceptModelChanged`, `$activateByEvent`, `$executeContributedCommand`).
5. Stub the rest (return empty / reject) so extensions that touch them do not hang. Full parity is ~1,000 methods; useful parity (language extensions, themes, simple commands) is perhaps 15-20 shapes.

Webview-based UI (webviews, custom editors, notebooks renderers) needs an embedded web view regardless of the native shell; defer.

---

## 5. Default theme and layout facts for the first window

### Theme files (`extensions/theme-defaults/themes/`)

Chain: `2026-dark.json` → includes `dark_modern.json` → `dark_plus.json` → `dark_vs.json`. Theme list in `extensions/theme-defaults/package.json`.

**Important:** the default dark theme in 1.147 is **"Dark 2026"** (`ThemeSettingDefaults.COLOR_THEME_DARK = 'Dark 2026'`, `src/vs/workbench/services/themes/common/workbenchThemeService.ts:43`), not Dark Modern. However `COLOR_THEME_DARK_INITIAL_COLORS` (same file, l.73, used for first paint before the theme loads) still holds the Dark Modern values.

| Key | Dark Modern (`dark_modern.json`) | Dark 2026 (`2026-dark.json`) |
|---|---|---|
| `editor.background` | `#1F1F1F` | `#121314` |
| `editor.foreground` | `#CCCCCC` | `#BBBEBF` |
| `foreground` | `#CCCCCC` | `#bfbfbf` |
| `focusBorder` | `#0078D4` | `#3994BCB3` |
| `sideBar.background` | `#181818` | `#191A1B` |
| `sideBar.foreground` / `sideBar.border` | `#CCCCCC` / `#2B2B2B` | `#bfbfbf` / `#2A2B2C` |
| `activityBar.background` | `#181818` | `#191A1B` |
| `activityBar.foreground` / `inactiveForeground` | `#D7D7D7` / `#868686` | `#bfbfbf` / `#8C8C8C` |
| `activityBar.activeBorder` / `border` | `#0078D4` / `#2B2B2B` | `#bfbfbf` / `#2A2B2C` |
| `activityBarBadge.background` | `#0078D4` | `#307E9F` |
| `statusBar.background` | `#181818` | `#191A1B` |
| `statusBar.foreground` / `border` | `#CCCCCC` / `#2B2B2B` | `#8C8C8C` / `#2A2B2C` |
| `statusBar.noFolderBackground` / `debuggingBackground` | `#1F1F1F` / `#0078D4` | `#191A1B` / `#3994BC` |
| `editorGroupHeader.tabsBackground` / `tabsBorder` | `#181818` / `#2B2B2B` | `#191A1B` / `#2A2B2C` |
| `tab.activeBackground` / `activeForeground` | `#1F1F1F` / `#FFFFFF` | `#121314` / `#bfbfbf` |
| `tab.inactiveBackground` / `inactiveForeground` | `#181818` / `#9D9D9D` | `#191A1B` / `#8C8C8C` |
| `tab.activeBorderTop` / `activeBorder` / `border` | `#0078D4` / `#1F1F1F` / `#2B2B2B` | `#3994BC` / `#121314` / `#2A2B2C` |
| `titleBar.activeBackground` / `activeForeground` | `#181818` / `#CCCCCC` | `#191A1B` / `#8C8C8C` |
| `titleBar.inactiveBackground` / `inactiveForeground` | `#1F1F1F` / `#9D9D9D` | `#121314` / `#8C8C8C` |
| `titleBar.border` | `#2B2B2B` | `#2A2B2C` |
| `panel.background` / `panel.border` | `#181818` / `#2B2B2B` | `#191A1B` / `#2A2B2C` |
| `editorLineNumber.foreground` / `activeForeground` | `#6E7681` / `#CCCCCC` | `#858889` / `#BBBEBF` |

Colors not set by any theme fall back to defaults registered in `src/vs/platform/theme/common/colors/*.ts` and `src/vs/editor/common/core/editorColorRegistry.ts`.

### Fonts (macOS)

- Editor: `Menlo, Monaco, 'Courier New', monospace`, **12px** on mac (14 on Windows/Linux), line height = `GOLDEN_LINE_HEIGHT_RATIO` × fontSize = **1.5** on mac (1.35 elsewhere) → 18px. Source: `src/vs/editor/common/config/fontInfo.ts:14,222-242`. Windows: `Consolas, 'Courier New', monospace`; Linux: `'Droid Sans Mono', monospace`.
- Workbench UI: `-apple-system, BlinkMacSystemFont, sans-serif`, **13px**, line-height 1.4em (`src/vs/workbench/browser/media/style.css:12,48-50`).

### Layout dimensions

`workbench.experimental.modernUI` defaults to **true** in 1.147 (`src/vs/workbench/browser/workbench.contribution.ts:834`, setting id in `src/vs/workbench/services/layout/browser/layoutService.ts:54`), which turns on "floating panels" and changes several sizes. Both sets:

| Element | Modern UI (default now) | Classic | Source |
|---|---|---|---|
| Activity bar width | 36 (compact 28) | 48 (compact 36) | `src/vs/workbench/browser/parts/activitybar/activitybarPart.ts:51-57` |
| Activity bar item height | 36 | 48 (compact 28) | same |
| Status bar height | 22 + 6 bottom padding (compact +4) | 22 | `src/vs/workbench/browser/parts/statusbar/statusbarPart.ts:124-132` |
| Editor tab row height | 32 (24 tab + 8 padding; compact 28) | 35 (compact 22) | `src/vs/workbench/browser/parts/editor/editorTabsControl.ts:106-115` |
| Title bar height | 35 with command center (default on), 30 without | same | `src/vs/platform/window/common/window.ts:448`, `src/vs/workbench/browser/parts/titlebar/titlebarPart.ts:236` |
| Sidebar / panel / aux bar initial size | 300 | 300 | `src/vs/workbench/browser/layout.ts:2879-2881` |

Title bar style defaults to **custom** on all OSes (`getTitleBarStyle`, `src/vs/platform/window/common/window.ts:408-432`); on macOS it falls back to native only with `window.nativeTabs` or `window.nativeFullScreen: false`. Command center defaults to on (`window.commandCenter`, `workbench.contribution.ts:917`).

Recommendation for the first window: classic dimensions (48/22/35/35) with Dark Modern colors are simpler and match `COLOR_THEME_DARK_INITIAL_COLORS`; switch to Dark 2026 + modern UI sizes once the layout grid exists.
