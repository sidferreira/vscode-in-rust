# Extension compatibility plan for rscode

Status: research, 2026-11-09. Scope: **desktop macOS (darwin-arm64) with the Node extension host only**.
Browser / web-extension entry points (`browser` manifest field, `dist/browser/*`, `vscode.dev`-style
hosting) are **not planned**; they are mentioned below only where they inflate scanner totals.

Extensions were read, never executed, never copied into this repo. Every number below comes from
`scripts/ext-api-scan` (regenerate with `scripts/ext-api-scan ~/.vscode/extensions/<folder>... --md out.md`)
or from a grep noted in the text. Paths are inside `~/.vscode/extensions/<folder>/` unless they start
with `vscode-original/` (upstream, MIT, read-only at `../vscode-original`, v1.147.0).

## 0. How to read the numbers (limits of the scanner)

- **exact** = `vscode.<ns>.<member>` reached through a binding of `require("vscode")` (const/var, `__toESM`, ESM import, destructuring) and checked against the upstream `vscode.d.ts` surface.
- **heuristic** = `.<ns>.<fn>(` on an object that is not a known binding. This is how webpack externals appear (`o.window.createWebviewPanel(`), and it is the **only** count for PR, GitLens, ErrorLens and Kotlin (their bundles hide the alias). Treat heuristic totals as "roughly how many calls", not exact.
- **Top-level module functions** (e.g. `createCommentController`) are counted exact only when reached through an alias. Webpack-external bundles (PR: 4 hits by grep) show 0 in the scanner. Use grep for those names.
- Bundles under `dist/browser/` and `browser` entries are counted in the totals but are out of scope.
- `window.*`, `workspace.*` etc. collisions with DOM globals are filtered by the d.ts whitelist; members not in the d.ts land in `unknown`.

## 1. Per-extension profiles

### 1.1 anthropic.claude-code 2.1.319 (darwin-arm64) — installed today

- **What it does.** Claude Code IDE integration: a webview panel/side view that talks to the `claude` CLI, plus terminal, diff, and tab integration.
- **Entry.** Node: `extension.js` (3.5 MB, one ext-host bundle; `package.json` `main`). No `browser` field.
- **Activation.** `onStartupFinished`, `onWebviewPanel:claudeVSCodePanel` (`package.json`).
- **Contributions.** commands 31, configuration 23, menus 28, keybindings 9, viewsContainers 3, views 3, walkthroughs 1, jsonValidation 1.
- **Top APIs (exact).** `commands.executeCommand` 55, `commands.registerCommand` 32, `window.tabGroups` 28, `workspace.workspaceFolders` 23, `workspace.getConfiguration` 22, `window.showInformationMessage` 19, `workspace.openTextDocument` 12, `window.activeTextEditor` 10, `env.remoteName` 7, `workspace.registerFileSystemProvider` 4.
- **Webviews.** `createWebviewPanel` 2, `registerWebviewViewProvider` 3, `registerWebviewPanelSerializer` 1; one webview bundle (`webview/index.js`, React, 1 `acquireVsCodeApi()`).
- **Terminal.** `createTerminal` 2, `activeTerminal` 1, `onDidStartTerminalShellExecution` / `onDidEndTerminalShellExecution` / `onDidChangeTerminalShellIntegration` (shell integration events).
- **Other.** `registerUriHandler` 1, `env.clipboard` 5, `window.activeNotebookEditor` 1, notebook types (`NotebookCellData`, `NotebookEdit`, ...), `CommentThreadCollapsibleState` 2 and `createCommentController` (grep: 1), `TabInputWebview` 3, `TabInputTerminal` 1.
- **Proposed APIs.** None (`enabledApiProposals` absent).
- **Native processes.** It launches the bundled CLI `resources/native-binary/claude` (Mach-O arm64, ~322 MB). The extension resolves it with `asAbsolutePath("resources","native-binary", …)` (grep of `extension.js`) and requires `child_process` in its bundle. Also `resources/audio-capture/arm64-darwin/audio-capture.node` (Mach-O, 438 KB, microphone for voice input). The `resources/native-binaries/win32-x64` branch is irrelevant here.
- **LSP.** No.

### 1.2 github.vscode-pull-request-github 0.176.1

- **What it does.** Pull requests and issues: PR list/review UI, PR description webviews, comments on diffs, Copilot/LM chat tools, file system providers for `githubpr:`/`pr:` URIs.
- **Entry.** Node: `dist/extension.js` (2.3 MB); also 3 node chunks; browser bundle `dist/browser/extension.js` (out of scope).
- **Activation.** 12 events, incl. `onStartupFinished`, `onOpenExternalUri:https`, `onFileSystem:pr`, `onWebviewPanel:PullRequestOverview`, `onChatContextProvider:githubpr`.
- **Contributions.** commands 175, menus 369, configuration 75, viewsWelcome 21, views 11, viewsContainers 2, languageModelTools 9, chatSkills 6, chatContext 2, colors 9, keybindings 6, resourceLabelFormatters 1, yamlValidation 2.
- **Dependencies.** `extensionDependencies: ["vscode.github-authentication"]` (built-in, upstream `extensions/github-authentication`). Built-in refs in bundle: `vscode.git` (4 string refs; `extensions.getExtension("vscode.git")` at `dist/extension.js`), `vscode.diff` 18.
- **Auth.** `authentication.getSession` 4, `authentication.getAccounts` 2.
- **Webviews.** `createWebviewPanel` 4, `registerWebviewViewProvider` 6, `registerWebviewPanelSerializer` 4; 3 webview bundles (`dist/webview-*.js`).
- **Tree/FS.** `createTreeView` 16, `registerFileSystemProvider` 16, `registerTextDocumentContentProvider` 2, `openTextDocument` 14, `createQuickPick` 20.
- **Comments.** `createCommentController` (grep: 4 in `dist/extension.js`) plus comment proposals.
- **Chat/LM.** `lm.registerTool` 18, `lm.selectChatModels` 2, `chat.registerChatAttachContextProvider` 4, `chat.registerChatTabContextProvider` 4 (all heuristic).
- **URI/external.** `registerUriHandler` 2, `env.openExternal` 22, `env.asExternalUri` 8.
- **Proposed APIs: 32 enabled** (`package.json` `enabledApiProposals`), including `activeComment`, `commentReactor`, `commentReveal`, `commentThreadApplicability`, `chatContextProvider`, `chatParticipantPrivate`, `chatSessionsProvider`, `quickDiffProvider`, `diffCommand`, `shareProvider`, `tokenInformation`, `remoteCodingAgents`, `treeViewMarkdownMessage`. Used in code (heuristic): `quickDiffProvider` 2, `diffCommand` 6, `chatContextProvider` 8, `shareProvider` 2, `tokenInformation` 2.
- **Native processes.** None. No `child_process`.
- **LSP.** No.

### 1.3 eamodio.gitlens 19.5.0

- **What it does.** Git blame/history/lens, commit graph, compare, worktrees, launchpad, AI commit-message helpers; many webviews (graph, patch details, settings, timeline, welcome).
- **Entry.** Node: `dist/gitlens.js` (3.2 MB) plus 13 node chunks; 18 webview bundles (`dist/webview-*.js`); browser bundle (out of scope).
- **Activation.** 8 events incl. `onWebviewPanel:gitlens.graph`, `onTerminal:*`, `onUri`, `onFileSystem:gitlens`.
- **Contributions.** commands 1172, menus 3466, configuration 468, colors 76, icons 94, keybindings 59, viewsWelcome 61, views 21, viewsContainers 4, customEditors 1, mcpServerDefinitionProviders 1, walkthroughs 1, configurationDefaults 2.
- **Heuristic APIs (all of it; no exact aliases).** `l10n.t` 7684 (localisation, cheap), `window.*` messages/quick picks/`withProgress`/`createTextEditorDecorationType` 16/`createTreeView` 4/`createWebviewPanel` 2 (+ serializer 2, custom editor 2, view provider 2), `workspace.registerFileSystemProvider` 4, `createFileSystemWatcher` 2, `workspace.openTextDocument` 12, `createOutputChannel` 5, `createStatusBarItem` 12.
- **Terminal/tasks.** `createTerminal` 2, `registerTerminalLinkProvider` 2, `tasks.fetchTasks`/`executeTask` 2 each.
- **Dependencies.** `extensions.getExtension` 11; built-in refs `vscode.github-authentication` 2, `vscode.git` 1, `vscode.diff` 4. `authentication.getSession` 2.
- **LM / MCP.** `lm.selectChatModels` 4, `lm.registerMcpServerDefinitionProvider` 1.
- **Native.** One file with `child_process` (~101 spawn-like call sites, heuristic). Most likely git CLI. Not verified in source here, so treat as "spawns git (assumed)".
- **Proposed APIs.** None.
- **Deps not measured.** `@gitkraken/commit-graph` and `@gitkraken/commit-graph-ui` are declared in `package.json` and may ship WASM/native code. Not checked.

### 1.4 usernamehw.errorlens 3.30.0

- **What it does.** Renders diagnostics, git-blame-like inline text and the first message at the end of each line (decorations); status bar summary.
- **Entry.** `dist/extension.js` (292 KB total folder). Its `browser` field points to the same file, so the scanner labels it "browser"; it is the desktop entry too.
- **Activation.** `onStartupFinished`. `extensionKind: ["ui","workspace"]`.
- **Contributions.** commands 17, configuration 77, colors 30 (theme colours). No views, menus or languages.
- **APIs (heuristic, one file).** `createTextEditorDecorationType` 15 (after/before text is the core feature; the decoration option shape was not checked), `languages.getDiagnostics` 16, `window.createStatusBarItem` 4, `commands.registerCommand` 17, `registerTextEditorCommand` 3, `workspace.getConfiguration` 8, `window.showQuickPick` 3, `languages.registerCodeLensProvider` 1, `workspace.findFiles` 1, `workspace.openTextDocument` 2, `env.openExternal` 1.
- **Not used.** Webviews, LSP, native code, proposed APIs, `child_process`.
- **Note.** The best first milestone: the smallest surface, and it uses mostly the parts rscode needs anyway.

### 1.5 oxc.oxc-vscode 1.64.1

- **What it does.** Oxc language server client: lint diagnostics, code actions, formatting, for JS/TS and many languages (21 activation languages).
- **Entry.** `out/main.js` (477 KB), node. `package.json` `main` = `./out/main.js`.
- **Activation.** 21 `onLanguage:*` events (`astro`, `css`, `html`, `javascript`, `json`, `markdown`, ...).
- **Contributions.** commands 8, configuration 24, jsonValidation 2, languages 1.
- **LSP.** **Yes.** `vscode-languageclient` (declared dependency; markers in `out/main.js`, `TransportKind` etc.).
- **APIs (exact, 183 calls).** `workspace.getConfiguration` 14, `languages.match` 14 (document selectors), `workspace.workspaceFolders` 10, `commands.registerCommand` 9, `window.showErrorMessage` 8, `workspace.fs` 6, `workspace.textDocuments` 6, `workspace.notebookDocuments` 5 and `onDid{Open,Change,Save,Close}NotebookDocument` (notebook sync for LSP), `languages.createDiagnosticCollection` 3, `workspace.findFiles` 3, `workspace.isTrusted` 3, `languages.registerCodeActionsProvider` 2, `registerSignatureHelpProvider` 2, `registerDocumentSemanticTokensProvider` 1, `registerFoldingRangeProvider` 1, `registerInlayHintsProvider` 1, `registerDefinitionProvider` 1, `registerDocumentFormattingEditProvider` 1, `registerCompletionItemProvider` 1, `registerCodeLensProvider` 1, `registerHoverProvider` 1.
- **Native.** No bundled server binary in the install (`out/` has only `main.js`). The language server is resolved at runtime (presumably from the workspace's `node_modules` or PATH); **not verified** in the source here.
- **Webview/proposed/process.** None; no `child_process` in the bundle.

### 1.6 redhat.vscode-xml 0.29.3 (darwin-arm64)

- **What it does.** XML/XSD/DTD language support (lemminx): validation, completion, formatting, symbols, catalogs.
- **Entry.** `dist/extension.js` (707 KB) + `dist/540.extension.js` (308 KB), node.
- **Activation.** `onLanguage:xml|xsl|dtd|svg`, two `onCommand:xml.open.docs*`.
- **Contributions.** languages 2, grammars 2, configuration 55, configurationDefaults 4, commands 9, menus 11, jsonValidation 1.
- **LSP.** **Yes** (`vscode-languageclient`).
- **Native.** Both shipped: `server/lemminx-osx-aarch_64` (Mach-O, 47.7 MB, native image) and `server/org.eclipse.lemminx-0.31.2-uber.jar` (8.8 MB). The bundle has `lemminx-osx-aarch_64` and `lemminx-linux-*` names, so it picks the native binary on mac arm64 (grep). The jar is the fallback. Spawns `child_process` (~41 spawn-like sites, heuristic).
- **APIs (heuristic).** `workspace.getConfiguration` 57, `commands.executeCommand` 24, `languages.createDiagnosticCollection` 3, `languages.registerCompletionItemProvider` and friends (1–2 each), `workspace.applyEdit` 4, `extensions.getExtension` 3, `createWebviewPanel` 1 (docs page), `showInputBox` 2, `showQuickPick` 2, `withProgress` 2.

### 1.7 ms-vscode.vscode-typescript-next 6.0.20260416

- **What it does.** Ships the TypeScript compiler and a grammar set used by VS Code's built-in TypeScript language features (`tsserver`). It is not an extension host program.
- **Entry.** None: no `main`, no `browser`, no activation events. `extensionKind: ["workspace"]`.
- **Contributions.** grammars 4 (`syntaxes/*.tmLanguage.json`) only.
- **Runtime.** `node_modules/typescript/bin/tsserver` and `tsc` (shell-script style, executed through Node). The built-in `typescript-language-features` extension finds this install by its extension id: `vscode-original/extensions/typescript-language-features/src/tsServer/versionProvider.electron.ts` contains the `vscode-typescript-next` reference.
- **Consequence.** rscode needs the built-in TypeScript feature extension (compiled from upstream) plus `tsserver` spawned as a child process. The extension itself is data (grammars and the TS lib).

### 1.8 fwcd.kotlin 0.2.36

- **What it does.** Kotlin language support via the Kotlin language server (JVM) and a debug adapter.
- **Entry.** `dist/extension.js` (1.1 MB), node.
- **Activation.** `onLanguage:kotlin`, `onDebugResolve:kotlin`.
- **Contributions.** languages 1, grammars 2, commands 1, menus 1, breakpoints 1, debuggers 1, configuration 30.
- **LSP.** **Yes** (`vscode-languageclient`).
- **Runtime download.** The bundle references `api.github.com/repos/fwcd/…` (grep), so it downloads the language server at runtime rather than shipping it. It requires a Java runtime (`kotlin.java.home` setting, `"java"` literals in the bundle; heuristic).
- **DAP.** `debug.registerDebugAdapterDescriptorFactory` 1, `debug.startDebugging` 2.
- **APIs (heuristic).** `languages.createDiagnosticCollection` 3, `languages.match` 13, `languages.registerCodeLensProvider` 2, `registerSignatureHelpProvider` 2, `registerDocumentSemanticTokensProvider` 1, `registerFoldingRangeProvider` 1, `registerInlayHintsProvider` 1, `workspace.getConfiguration` 12, `window.showWarningMessage` 6, `window.createOutputChannel` 2, `workspace.createFileSystemWatcher` 2.

## 2. Capability matrix

Key: **R** required for the extension to work; **O** optional or minor; **–** not used. Counts are the scanner's totals (exact + heuristic) unless marked grep. Columns: CC = claude-code, PR = pull-request-github, GL = gitlens, EL = errorlens, OX = oxc, XM = vscode-xml, TS = typescript-next, KT = kotlin.

| rscode capability | CC | PR | GL | EL | OX | XM | TS | KT | Notes |
|---|---|---|---|---|---|---|---|---|---|
| Node ext host process + RPC (`vsr-rpc`) | R | R | R | R | R | R | – | R | TS needs the built-in TS extension host side instead |
| Commands / keybindings / menus | R (31) | R (175) | R (1172) | R (17) | R (8) | R (9) | – | R (1) | Menus are declarative, pure data |
| Configuration (`vsr-settings`) | R (23) | R (75) | R (468) | R (77) | R (24) | R (55) | – | R (30) | `getConfiguration` 14–57 calls per ext |
| Status bar items | O (1) | R (4) | R (12) | R (4) | O (1) | – | – | O (1) | |
| Tree views / view containers | – (webview views) | R (16 tree) | R (4 tree) | – | – | – | – | – | CC uses webview views (3) |
| Decorations (`TextEditorDecorationType`, after/before text) | O (1) | R (6) | R (16) | R (15) | – | – | – | – | EL's core feature |
| Diagnostics (`DiagnosticCollection`, `getDiagnostics`) | – | – | – | R (16 heur) | R (3) | R (3) | – | R (3) | |
| Language features via providers (completion, hover, codeaction, codelens...) | – | O (completion 8) | O (hover 8) | O (codelens 1) | R (~10) | R (~10) | – | R (~10) | Providers are the main-side surface |
| LSP client (`vscode-languageclient`, stdio child process) | – | – | – | – | **R** | **R** | (TS uses tsserver, not LSP) | **R** | |
| Text document sync (`openTextDocument`, change events, notebook docs) | R (12) | R (14) | R (12) | O (2) | R (+notebook sync 5) | R (1) | – | R | LSP needs full sync |
| `workspace.fs` / file system provider / watchers | R (FSP 4) | R (FSP 16) | R (FSP 4, watcher 2) | O (findFiles 1) | R (fs 6, watcher 2) | O (watcher 1) | – | O (watcher 2) | |
| SCM API + built-in `vscode.git` API | – | R (git ext 4 refs, quickDiff 2) | O (git ext 1; CLI git assumed) | – | – | – | – | – | Built-in git extension must exist |
| Authentication providers (GitHub) | – | R (getSession 4) | O (getSession 2) | – | – | – | – | – | PR depends on `vscode.github-authentication` |
| Comments API | R (grep 1) | R (grep 4) | – | – | – | – | – | – | Proposed comment APIs for PR |
| Webview panels / views / custom editors | R (2+3+1) | R (4+6+4) | R (2+2+2+2; 18 bundles) | – | – | O (1 docs) | – | – | Needs embedded webview (see §3.4) |
| Terminal API (create, shell integration, link provider) | R (2 + shell events) | O (link 2) | O (create 2, link 2) | – | – | – | – | – | Needs pty + shell integration |
| Tasks API | – | – | O (2) | – | – | – | – | – | |
| Notebooks API | O (type refs) | – | – | – | O (sync events 5) | – | – | – | |
| Chat / LM APIs (`lm`, `chat`, MCP) | – | R (lm.registerTool 18, chat 8) | O (lm 4, MCP 1) | – | – | – | – | – | Chat contributions are data |
| Proposed APIs (proposal gating) | – | **R (32 enabled)** | – | – | – | – | – | – | Proposal allowlist is per product config |
| Uri handler / `env.openExternal` / `asExternalUri` | O (1) | R (2; 22; 8) | R (2; 8; 10) | O (1) | O (1) | O (1) | – | O (1) | Auth callbacks depend on it |
| Native binaries / child processes | R (CLI binary, audio .node) | – | O (git CLI, assumed) | – | R (LSP server, runtime) | R (lemminx native/jar) | R (tsserver) | R (Java + LS download) | |
| Debug adapter (DAP) | – | – | – | – | – | – | – | R (1) | Later milestone |
| Theme color / icon / label contributions | – | O (colors 9) | O (colors 76, icons 94) | O (colors 30) | – | – | – | – | Declarative |

## 3. How to run unmodified extensions

The goal (ADR-0001) is unmodified extensions in a Node extension host that talks to a Rust "main thread". Three ways to get the host code:

### 3.1 Option A: run upstream's compiled extension host (`extensionHostProcess`) from a self-built Code-OSS

- **Protocol.** Upstream's ext host code is the reference implementation of the RPC and API semantics (`src/vs/workbench/api/common/extHost*.ts`, `src/vs/workbench/api/node/extensionHostProcess.ts`, `src/vs/workbench/services/extensions/common/rpcProtocol.ts`, `extHost.protocol.ts`). Rust implements the MainThread side (`vsr-rpc`).
- **Build.** Code-OSS compiles from `vscode-original` (MIT). Requirements:
  - `pnpm install` plus `npm run compile` in a separate copy, not in place (ADR-0002 forbids building in `vscode-original`; ADR-0004 says the disk filled up before).
  - The package proxy quarantines some versions (ADR-0007/0012). Expect 403s on installs and step-backs. **Not tried here.**
  - The built-in extensions that PR and GitLens depend on must be compiled too: `extensions/git`, `extensions/github-authentication`, `extensions/typescript-language-features`, `extensions/git-base`, `extensions/diff` (`vscode.diff` is a command used by PR and GitLens).
- **Gains.** `vscode` module semantics (types, converters, events, proposals gating), manifest handling, activation, all the built-in extension behaviour PR relies on. This is the only route that does not re-implement ~21.5k lines of API.
- **Costs.** Disk, build time, and a large main-side protocol surface: ~172 `createProxyIdentifier` calls at `extHost.protocol.ts` (architecture-map §4 counts 89 MainThread and 83 ExtHost shapes; the grep I ran on this file found fewer shape declarations, so the count needs a proper generator). Wire ids are declaration-order (`proxyIdentifier.ts:33-52`), so a generated table is needed.

### 3.2 Option B: reuse the installed VS Code app bundle

- `architecture-map.md` §4 item 1 proposes `/Applications/Visual Studio Code.app/Contents/Resources/app/out/` as the JS source. **I do not recommend this, and I'm flagging it against that map.** The Microsoft-distributed product is not MIT (the VS Code product licence and the Microsoft product terms apply, not the repo's MIT source license). Unless the user decides otherwise, do not ship or depend on it. It may still be fine for local experiments, if the user says so.

### 3.3 Option C: our own minimal `vscode` shim in JS

- Write a Node module `vscode` that exposes only what the target extensions touch, backed by our own protocol.
- **Pros.** No Code-OSS build, no MIT/Microsoft question, small start. Sufficient for ErrorLens and oxc (about 30 members each).
- **Cons.** Built-in extensions must be replaced: `vscode.git` (PR, GitLens), `vscode.github-authentication` (PR, GitLens), `typescript-language-features` (ts-next). Each replacement is a re-implementation. Behaviour drift against upstream on Uri/Range/event/`Disposable` semantics, proposal gating, and the many small converters grows with every extension. PR and GitLens would need thousands of calls covered.

### 3.4 Webviews (affects Claude Code, PR, GitLens, XML docs)

- Webview extensions need an embedded web view inside the gpui window. ADR-0001's hybrid escape hatch: `wry` on macOS wraps `WKWebView` in a child view. Desktop macOS only, so WKWebView is acceptable (no cross-platform concern now).
- Webview HTML/JS (`webview/index.js`, `dist/webview-*.js`) talks through `acquireVsCodeApi()` `postMessage`; the host side is `extHostWebviewPanels.ts` / `extHostWebview*.ts` upstream. The main-side webview service is a separate milestone.

### 3.5 Recommendation

**Option A (upstream ext host from a self-built Code-OSS of the pinned v1.147.0), with Rust on the main side.** Reasoning:

1. Fidelity matters most for PR and GitLens (thousands of API calls, 32 proposals, built-in git and auth extensions). Only the upstream ext host gets that for free.
2. A shim (C) would need the built-in extensions re-implemented, which is the largest risk. Option C is still a fine throwaway for milestone 1 if Option A's build is blocked.
3. Option B is out because of licensing.
4. Option A's costs are one-off (build, disk) and the protocol generator, which we need anyway (ADR-0003 tracking).

Before committing: prove the build works through the proxy on a separate volume (a 1-hour spike). If it cannot, fall back to C for ErrorLens and oxc only.

## 4. Phased roadmap (cost/benefit order)

Effort is rough and assumes Option A. Dependencies first.

| Phase | Goal | Needs | Rough effort |
|---|---|---|---|
| P0 | Ext host spike: build Code-OSS elsewhere, run `extensionHostProcess` under Node, complete handshake (`Ready` → `IExtensionHostInitData` → `Initialized`) | Build spike; `vsr-rpc` framing (`ipc.net.ts` `PersistentProtocol`, 13-byte header); `rpcProtocol.ts` MessageBuffer codec; generated id table from `extHost.protocol.ts` | M-L |
| P1 (**first milestone: ErrorLens**) | Activate ErrorLens, register 17 commands, read config, status bar, decorations, diagnostics | MainThread shapes: ExtensionService, Commands, Configuration, DocumentsAndEditors/Documents/TextEditors (mirror models), Diagnostics, StatusBar, TextEditorDecorations, Workspace (minimal), MessageService, Storage, Logger | M |
| P2 | oxc (LSP over stdio) | LanguageClient plumbing needs child processes on main side; LanguageFeatures shapes (completion, hover, codeaction, codelens, semantic tokens, folding, inlay, signature, formatting, definition); notebook doc sync; workspace.fs; `languages.match` | L |
| P2b | TypeScript language features (for TS/JS) | Compile `typescript-language-features`; tsserver child process; ts-next install discovery | M |
| P3 | xml (LSP + native lemminx) | Same as P2; native binary spawn; catalogs and docs webview (defer docs webview) | M (after P2) |
| P4 | Kotlin (LSP + runtime LS download + Java + DAP) | P2 + Java detection + DAP shapes (`MainThreadDebugService`) | L |
| P5 | GitLens | Webviews (18 bundles, custom editor), tree views, terminals, tasks, git (`vscode.git` built-in + CLI), many commands/menus (1172/3466 contributions: declarative, cheap) | XL |
| P6 | **Claude Code** | Webview view + panel (`registerWebviewViewProvider`, `createWebviewPanel`, serializer), terminal with shell integration, `registerUriHandler`, `window.tabGroups` model, `TabInputWebview`, native `claude` spawn from ext host (child_process), audio `.node` (microphone permission on macOS), comments/notebook bits | XL (blocked on webview service and pty) |
| P7 | **PR GitHub** | `vscode.github-authentication` compiled and wired to macOS keychain or our secret store; comment controller and proposed comment APIs; `vscode.git` API (quickDiff, SCM); webviews (3); tree views; FSP (`githubpr:`, `pr:`); LM tools; 32 proposals allowed by product config | XXL (last) |

Ordering reasoning: ErrorLens (P1) exercises the RPC, config, decorations and diagnostics without webviews, native code or LSP. oxc (P2) adds LSP and document sync, which unlock xml, Kotlin and TS. Claude Code and PR need the webview service, auth, git API and proposal gating, so they come last.

**Blockers, specifically.**
- **Claude Code:** webview service (WKWebView via wry), terminal pty + shell integration events, `native-binary/claude` must run from ext host child process (path resolution through `asAbsolutePath`), audio `.node` permissions, `registerWebviewViewProvider` used 3 times.
- **PR GitHub:** `vscode.github-authentication` (must compile and run), GitHub OAuth redirect via `registerUriHandler`, comment controller and proposed comment APIs, `vscode.git` API (SCM, quickDiff), 3 webview bundles, tree views with markdown labels (`treeItemMarkdownLabel` proposal), FSP for virtual documents, LM tools, product-level proposal allowlist (`product.json` `extensionEnabledApiProposals`; not located in `vscode-original` root in this check, so verify where upstream keeps it).

## 5. Test plan

Each milestone has these levels. Running installed extension code needs the user's go-ahead (this research did not run any).

- **Unit (no extension code).** Protocol codec round-trips against upstream fixtures: port `src/vs/workbench/services/extensions/test/common/rpcProtocol.test.ts` and `src/vs/base/parts/ipc/test/node/ipc.net.test.ts` (both present in `vscode-original`). Generated id table: a test that regenerates from the pinned `extHost.protocol.ts` and diffs against the committed table (fails on upstream drift). Manifest parsing: `scripts/tests/test_ext_api_scan.py` (21 tests, passing).
- **Headless ext host.** Spawn `extensionHostProcess` under Node with a Rust (or Node test double) main side. Feed `IExtensionHostInitData` pointing at a copy of the extension folder. Assertions per milestone:
  - P1 ErrorLens: `$activateByEvent("onStartupFinished")` observed; 17 `$registerCommand` calls; `$changedDiagnostics` from a seeded diagnostic produces a decoration call (`$trySetDecorations` / `$applyDecorations`) on the fake text document; status bar `$setStatusBarEntry` after activation.
  - P2 oxc: LSP initialise handshake over stdio to a fake server (a stdlib Python script in tests, not the real oxc binary); `publishDiagnostics` round-trip shows as `$changedDiagnostics`; document sync `$acceptModelChanged` on edit.
  - P6/P7: dialog-level tests are manual; headless assertions on `$createWebviewPanel` ids and `$registerTreeDataProvider` calls.
- **Regression on upgrade.** Re-run `scripts/ext-api-scan` on the new folder and diff the JSON profile (`api.exact`, `api.proposed`, `contributes`). New namespaces or proposals flag work before the extension is updated.
- **Smoke with the real window.** `scripts/rscode` on a fixture repo with ErrorLens enabled: seeded diagnostics render as end-of-line text. Visual check by the user (no screenshots from agents, per the repo rules).

## 6. Open items and not verified

- The `vscode-original` `out/` directory is absent (per ADR-0002); Option A's build has not been attempted. Disk and proxy behaviour are unknown.
- Proposal allowlist location in upstream (`product.json`) was not found at the `vscode-original` root in this check.
- Scanner limits: bundled webpack externals (PR, GitLens, ErrorLens, Kotlin) report heuristic counts only; top-level module functions (`createCommentController`) are missed in such bundles; `spawn`-like sites include false positives (e.g. `RegExp.exec`). The scanner classifies ErrorLens as "browser" only because its `browser` field points to the same file.
- The oxc language server location and the Kotlin Java discovery were not traced through the source.
- The GitLens native/git path (`child_process`) was not traced; the "git CLI" claim is an assumption.
- Browser/web entry points are intentionally not analysed (out of scope per the coordinator's scope decision: desktop macOS only, Node host).
