# Chat e2e scenarios

Upstream dir: `test/smoke/src/areas/chat` | pinned sha: `50e1350fc78d` | scenario count: 13
Suites (describe): Chat Model Configuration; Chat Sessions; Chat Sessions (Agent Host); Copilot CLI; Chat Sandbox (<platform>) with nested network/home-read suites; Chat Disabled

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | shows the schema defaults and every configured option in the model details page | Chat Model Configuration | `chat/chatModelConfig.test.ts:249` | Configures a model with options in settings and opens the model details page to verify schema defaults and each option are shown. | blocked-on-feature chat/agents; extension host | Uses a mock LLM server. |
| 2 | forwards the selected reasoning effort and context size to the server | Chat Model Configuration | `chat/chatModelConfig.test.ts:281` | Selects reasoning effort and context size in the model picker, sends a chat message, and inspects the mock server request. | blocked-on-feature chat/agents |  |
| 3 | Test ${session.name} session | Chat Sessions / Chat Sessions (Agent Host) | `chat/chatSessions.test.ts:305` | Opens a chat session (editor or view) by command, sends two messages tagged with mock scenarios, and asserts each mocked reply (Agent Host variant also asserts the session scheme). | blocked-on-feature chat/agents | 1 source it looped over sessions: Copilot CLI and Local in 'Chat Sessions', Agent Host Copilot in the Agent Host suite (3 runs at runtime). |
| 4 | Test ${shellSession.name} session run in terminal | Chat Sessions / Chat Sessions (Agent Host) | `chat/chatSessions.test.ts:371` | Opens a session, sends a prompt triggering a shell tool call with `echo`, and asserts the echoed marker and reply render in chat. | blocked-on-feature chat/agents; terminal | 1 source it looped over shell sessions (runtime count depends on suite config). |
| 5 | opens a Copilot CLI session and receives a response | Copilot CLI | `chat/copilotCli.test.ts:69` | Opens a Copilot CLI chat session against a mock LLM server and waits for the mocked response. | blocked-on-feature chat/agents | it.skip upstream. |
| 6 | runs terminal commands inside the sandbox | Chat Sandbox | `chat/chatSandbox.test.ts:232` | Asks the agent to run a command and verifies it executes inside the sandbox. | blocked-on-feature chat/agents; terminal sandbox | Platform-specific sandbox. |
| 7 | allows writing to the sandbox TMPDIR | Chat Sandbox | `chat/chatSandbox.test.ts:267` | Runs a terminal tool command writing to the sandbox TMPDIR and checks success. | blocked-on-feature chat/agents; terminal sandbox |  |
| 8 | blocks writing to the host temp directory | Chat Sandbox | `chat/chatSandbox.test.ts:292` | Runs a command writing to the host temp dir and checks it is denied. | blocked-on-feature chat/agents; terminal sandbox |  |
| 9 | allows terminal network access by default | Chat Sandbox | `chat/chatSandbox.test.ts:311` | Runs a network command through the sandbox and checks it succeeds. | blocked-on-feature chat/agents; terminal sandbox |  |
| 10 | blocks terminal network access | Chat Sandbox > with terminal network access disabled | `chat/chatSandbox.test.ts:337` | Disables sandbox network setting, runs the network command, checks it is blocked. | blocked-on-feature chat/agents; terminal sandbox |  |
| 11 | blocks reading files in the home directory outside the workspace by default | Chat Sandbox | `chat/chatSandbox.test.ts:365` | Asks the agent to read a home-directory file and checks it is denied. | blocked-on-feature chat/agents; terminal sandbox |  |
| 12 | allows reading a configured home directory file | Chat Sandbox > with home file read access enabled | `chat/chatSandbox.test.ts:400` | Configures an allowed home path, reads the file via the agent, checks success. | blocked-on-feature chat/agents; terminal sandbox |  |
| 13 | can disable AI features | Chat Disabled | `chat/chatDisabled.test.ts:15` | Sets `chat.disableAIFeatures` true in user settings, waits for the auxiliary bar to disappear, then lists commands matching chat/agent/copilot/mcp and asserts only one expected entry remains. | blocked-on-feature chat/agents (command palette, settings) | Needs settings editor, command palette, and the chat feature gate; the gate itself is the chat feature. |

## Notes

- 13 `it(` calls: modelConfig 2, chatSessions 2 (looped), copilotCli 1, chatSandbox 7, chatDisabled 1. Runtime count is higher because of the loops.
- Shared helper: shellScenarios.ts (shell echo scenario builder). All except chatDisabled start a mock LLM server via `getMockLlmServerPath`.
- 1 skipped (`it.skip`): Copilot CLI session.

## Soon vs far

Near-term (ready / pending):
- none

Blocked, grouped by feature:
- chat/agents; extension host: #1
- chat/agents: #2, #3, #5
- chat/agents; terminal: #4
- chat/agents; terminal sandbox: #6, #7, #8, #9, #10, #11, #12
- chat/agents (command palette, settings): #13
