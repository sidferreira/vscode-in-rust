# Agents Window e2e scenarios

Upstream dir: `test/smoke/src/areas/agentsWindow` | pinned sha: `50e1350fc78d` | scenario count: 9
Suites (describe): Agents Window (local AgentHost); Agents Window (Dev Container AgentHost) [conditional]; Agents Window (ssh|tunnel|wsl Dev Container AgentHost) [looped, conditional]; Agents Window (WSL AgentHost, Kerberos proxy) [conditional]; Editor Window (WSL remote AgentHost, Kerberos proxy) [conditional]; Agents Window (local AgentHost, SDK sandbox); Agents Window (Codex)

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | Replaces the new session UI with the in-progress AgentHost session | Agents Window (local AgentHost) | `agentsWindow/agentsWindow.test.ts:157` | Waits for the new-session view, picks the Copilot session type, submits a prompt tagged with a mock-LLM scenario, and waits for the active session view and the mocked reply. Optionally checks no git worktree was created. | blocked-on-feature chat/agents; agent host | Uses a mock LLM server. |
| 2 | Test Copilot CLI session via AgentHost | Agents Window (local AgentHost) | `agentsWindow/agentsWindow.test.ts:189` | Warms up the agent-host model, submits a prompt in a new session, waits for the mocked assistant reply and checks it. | blocked-on-feature chat/agents; agent host |  |
| 3 | Starts a session in a Dev Container | Agents Window (Dev Container AgentHost) | `agentsWindow/agentsWindow.test.ts:256` | Uses dev-container fixtures to start an agent-host session inside a container and verifies the reply. | blocked-on-feature remote (dev containers) | Suite only enabled when `runDevContainerSuite`; otherwise describe.skip. Fixtures in remoteDevContainerFixtures.ts (1115 lines). |
| 4 | Starts and reopens a Dev Container session over ${label} | Agents Window (<label> Dev Container AgentHost) | `agentsWindow/agentsWindow.test.ts:362` | Starts a dev-container session over the given transport, then reopens it and checks the session persists. | blocked-on-feature remote (dev containers) | Expands to 3 runs (ssh, tunnel, wsl) at runtime; 1 source it. Each transport is enabled by env flags, else describe.skip. Tunnel uses fixtures/tunnelAuthentication/extension.ts. |
| 5 | Uses the Windows proxy, Kerberos ticket and certificates from a WSL AgentHost | Agents Window (WSL AgentHost, Kerberos proxy) | `agentsWindow/agentsWindow.test.ts:437` | Starts an agents window against a WSL agent host and verifies proxy, Kerberos and certificate settings are inherited from Windows. | blocked-on-feature remote (WSL) and native policy/proxy | Windows-only; skipped unless `wslKerberosProxyScheme` is set. |
| 6 | Uses the Windows proxy, Kerberos ticket and certificates from the AgentHost of a WSL remote | Editor Window (WSL remote AgentHost, Kerberos proxy) | `agentsWindow/agentsWindow.test.ts:482` | Same proxy/Kerberos check but from an editor window attached to a WSL remote. | blocked-on-feature remote (WSL) and native policy/proxy | Windows-only; skipped unless `wslKerberosProxyScheme` is set. |
| 7 | Test Copilot CLI session via AgentHost (SDK sandbox) | Agents Window (local AgentHost, SDK sandbox) | `agentsWindow/agentsWindow.test.ts:531` | Runs the Copilot session flow with the SDK sandbox enabled and checks the mocked reply. | blocked-on-feature chat/agents; sandbox | May call this.skip() when the sandbox is unavailable. |
| 8 | recovers when the command palette loses focus before finding New Session | Agents Window (Codex) | `agentsWindow/agentsWindow.test.ts:622` | Installs a DOM listener that steals focus back to the chat input when the command palette receives the new-session command text, then runs startNewSession and asserts the interruption happened. | blocked-on-feature chat/agents | Needs command palette plus agents window UI. |
| 9 | Test Codex session | Agents Window (Codex) | `agentsWindow/agentsWindow.test.ts:654` | Checks Codex SDK availability (skips if absent), then submits a prompt and verifies the mocked Codex reply. | blocked-on-feature chat/agents; Codex SDK | Skips gracefully in builds without product.agentSdks.codex. |

## Notes

- 9 `it(` calls in one file. Several suites are conditional (`describe.skip` unless env flags set), so many are skipped in a default run.
- Shared helpers: `setupAgentHostSuite`, `warmUpAgentHostModel`, `dumpFailureDiagnostics`, plus fixtures/packagedAgentHost.ts, fixtures/tunnelAuthentication/extension.ts, remoteDevContainerFixtures.ts.
- Each suite starts a mock LLM server; scenarios are tagged in the prompt text.

## Soon vs far

Near-term (ready / pending):
- none

Blocked, grouped by feature:
- chat/agents; agent host: #1, #2
- remote (dev containers): #3, #4
- remote (WSL) and native policy/proxy: #5, #6
- chat/agents; sandbox: #7
- chat/agents: #8
- chat/agents; Codex SDK: #9
