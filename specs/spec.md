# Ariadna — native Git client for macOS with AI

Product specification. Meant to feed Spec-Driven Development and to let Javier (C# developer, macOS, personal use) iterate without rediscovering decisions.

Name: **Ariadna**. Bundle id: `es.javiermartinez.ariadna`. Minimum macOS: 15.0.

## 1. Vision and user

A native Git client in Swift and SwiftUI for the daily flow: status, staging by file and hunk, branches, merge, conflicts and remotes. It adds AI assistance that **never applies anything without confirmation**: commit messages, summaries, commit explanations and resolution proposals.

- MVP user: Javier, who already has GitHub Copilot (IDE + CLI) and Codex through a ChatGPT subscription.
- Value proposition: a single place for visual Git and the AI of those subscriptions, with no API keys and without sending the code to a new provider.
- Non-goals: multi-user SaaS, model hosting, replacing Xcode or VS Code, or reimplementing the full Copilot or Codex agent.

## 2. Scope

| Area | MVP (v0.1) | Later |
| --- | --- | --- |
| Repos | Open a local repo, status, stage by file/hunk, commit, log, diff, branches, fetch/pull/push | Interactive rebase, cherry-pick UI, stash UI, tags, submodules, LFS, worktrees |
| Conflicts | Detect, three-way model, editor, manual resolution, continue/abort | The AI proposes; Apply stays explicit |
| AI | Commit message, summary, explain commit, conflict proposal. Copilot / Codex / Auto selector | Copilot SDK, Codex app-server, repo chat |
| AI auth | Reuse `copilot login` and `codex login`. No API keys | In-app onboarding, OAuth if distributed |
| Distribution | Local build with SwiftPM / Xcode | Notarization, Sparkle, icon |

Hard constraint: nothing is written to the working tree, the index or the refs on the AI's initiative. Commit, merge, push and discarding changes are triggered by the user.

## 3. Git

Priority: parity with `git(1)` and zero index corruption.

- Open a folder; detect `.git`; recents and favorites.
- States: modified, untracked, staged, conflicted, ignored (with a toggle).
- Stage / unstage by file and by hunk. Discard worktree changes with confirmation.
- Commit the index. Empty message rejected. Amend is out of scope for this delivery.
- Log with parents, author, subject, body and decorations. Unified diff of worktree, index and commit.
- Branches: list, create, rename, delete, checkout. Detached HEAD visible.
- Merge `--ff`, `--no-ff` and `--ff-only`. Abort and continue for merge, rebase and cherry-pick.
- Remotes: list, add, fetch, pull (merge or rebase), push with `--set-upstream` if the upstream is missing. Remote auth is the system's (SSH agent / credential helper). It is not reimplemented.
- Conflicts: index stages (`:1:` base, `:2:` ours, `:3:` theirs) plus worktree markers (diff3 style too). Take one side per file or per hunk. Write the result only with confirmation. Marking as resolved is a separate `git add`.

## 4. AI

Every action produces an `AISuggestion` in the `pending` state. The user accepts, copies or discards it. Accepting does not call git.

| Capability | Input | Output |
| --- | --- | --- |
| Commit message | Staged diff and optional template | Editable subject + body |
| Summary | Diff | Bullets or PR style |
| Explain commit | SHA + patch | Text |
| Resolve conflict | Ours / base / theirs + path | Proposed file + reason |

Rules:

1. No write to the worktree, index or refs without an explicit button.
2. The context has a byte cap (100_000 by default). If it is exceeded, the CLI is not called.
3. Dry-run conflicts: the proposal is shown in the editor before the file is written.
4. The UI shows the provider and a summary of the command, without dumping the prompt (it may contain code and ends up in `argv`).
5. If the CLI is missing or not logged in, the error says what to run: `copilot login` or `codex login`.

Actual invocation (without giving them write permission):

- `copilot -p <prompt> -s --no-ask-user --deny-tool shell --deny-tool write …`
- `codex exec --json --sandbox read-only --skip-git-repo-check <prompt>`

The diff goes inside the prompt. `--allow-tool='shell(git:*)'` is not used: it would let Copilot run git. The AI process starts outside the repo. Codex runs in a read-only sandbox.

There are no API keys. The session is the one the user already created with the CLI. The subprocess environment is reduced: PATH, HOME, locale, SSH agent and `GIT_*` / `SSH_*` variables that do not look like tokens. The app does not copy `~/.codex/auth.json` or Copilot tokens.

## 5. Architecture

```
SwiftUI  →  AppModel / RepositorySession
                 ├─ GitCLIEngine : GitServing  →  ProcessRunner  →  git
                 └─ AIOrchestrator : AIProviding
                       ├─ CopilotCLIProvider  →  copilot
                       └─ CodexExecProvider   →  codex
```

- UI: SwiftUI, Observation, async/await. macOS 15+. AppKit only for the open-folder panel. The AI layer is only compiled with `ARIADNA_AI=1` (Phase 3). Merge and the three-way editor are part of the core and are always compiled.
- Persistence: JSON in Application Support (macOS) or `~/.config/ariadna` (Linux). No account of its own.
- Git through the CLI, not libgit2. Porcelain v2 (`git status --porcelain=v2 -z`), unified diff, `git log` with ASCII separators.
- `git` calls run serially per engine so the index is never clobbered. The AI does not enter that queue.
- Tests: XCTest, real temporary repos for git, and a `ProcessRunning` double for the CLIs.
- MVP distribution: outside the Mac App Store. The store sandbox prevents launching the system `git`, `copilot` and `codex` with the user's keychain and SSH agent. Hardened Runtime and notarization are left for the distribution phase.

## 6. Model

`Repository`, `GitStatus`, `StatusEntry`, `Commit`, `Branch`, `Remote`, `FileDiff`, `DiffHunk`, `ConflictFile` (ours / base / theirs), `ConflictHunk`, `AISuggestion` (`pending | accepted | rejected`), `AISettings`, `RecentRepository`.

## 7. Screens

- Welcome: path, Browse, recents, favorites.
- Three-column workspace: branches and remotes | changes, history or conflicts | diff or conflict studio.
- Commit bar: editor, Generate, Summarize, Create commit (⌘↩).
- Conflict studio: three panes + result. Use a side, a hunk, ask for a proposal, write (confirmation), mark resolved.
- History: local filter over the loaded commits, explanation on demand.
- Settings: provider, binary paths, context cap, pull with rebase.
- Proposal sheet: editable text, copy, regenerate, discard. The notice makes it clear the repo has not changed.

Shortcuts: ⌘O back to the welcome screen, ⌘↩ commit. The UI is in Spanish. `git` messages are shown as the CLI emits them.

## 8. Non-functional requirements

- Long operations with timeout and cancellation.
- No code telemetry. Copilot runs with `--disable-client-telemetry` (it does not affect GitHub's server-side telemetry).
- Apple Silicon as the distribution target. The core also builds on Linux so it can be tested without a Mac.
- Accessibility: labels in the bar, in the conflict panes and in the proposal.

## 9. Risks

| Risk | Mitigation in the MVP |
| --- | --- |
| App Sandbox vs external CLIs | Outside the Mac App Store. Documented. |
| Copilot or ChatGPT quota | The error is shown. No automatic retries. |
| Copilot CLI license if embedded | Not redistributed. The user installs it. |
| Unofficial proxies | Forbidden. Official CLIs only. |
| Unusual hooks and credential helpers | `git` runs them. stderr is shown. |

## 10. Phases

0. Spec in the repo, plan and tasks.
1. Usable Git core.
2. Manual conflicts with a three-way model.
3. AI for commit and summary, with a confirmation sheet.
4. Conflict proposal and commit explanation.
5. Polish and notarized distribution.

## 11. Decisions closed for this delivery

1. Name Ariadna. Bundle id `es.javiermartinez.ariadna`.
2. Minimum macOS 15. Swift tools 6.0.
3. One window, one repo. Recents are saved.
4. No clone or init.
5. Neither the Copilot nor the Codex binary is embedded.
6. CLI only. SDK and `codex app-server` later.
7. Pull requests in the browser, not in the app.
8. System theme.
9. Optional commit template, in the bar's text box.
10. Worktrees out.

## 12. Sources

Official routes, no API keys: programmatic Copilot CLI, Copilot CLI authentication, non-interactive `codex exec`, Codex login with ChatGPT, and github/spec-kit. Do not use unofficial proxies or GitHub Models (retired).
