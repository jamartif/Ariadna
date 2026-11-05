# MVP plan (foundation)

Derived from [spec.md](spec.md). This delivery builds the foundation that can already be compiled and tested, plus the SwiftUI UI ready to open in Xcode on a Mac. It is not yet the phase 5 polish.

## Design decisions

- **Git through the CLI.** `GitCLIEngine` implements `GitServing` by running `git`. Same semantics as the terminal, hooks included. libgit2 is ruled out until a `status` profile on huge repos calls for it.
- **Serial processes.** `SerialExecutor` chains an engine's `git` calls, so the index is never corrupted. The AI does not use that queue.
- **The AI only proposes.** `AIProviding` returns an `AISuggestion` in `pending`. `AIOrchestrator.accept` changes the state in memory. Writing the file requires `confirmed: true` in the engine, and committing is a separate method. There is no `apply` that joins both.
- **The user's CLIs.** Copilot: `copilot -p … -s --no-ask-user` with `shell` and `write` denied. Codex: `codex exec --json --sandbox read-only`. No API keys. The AI's cwd is not the repo.
- **Linux can test the core.** SwiftUI and AppKit are behind `#if os(macOS)`. On Linux the `Ariadna` executable only explains that the UI is Mac-only.
- **Swift tools 6.0, Swift 6 language mode, macOS 15.** On a Mac the manifest pins `.macOS(.v15)`. On Linux no platform is listed because SwiftPM does not accept an empty list.
- **AI behind a flag.** `ARIADNA_AI=1` includes the `AriadnaAI` target. By default the Phase 1 build does not link it. The code is not deleted. Merge and conflicts stay in `GitCore` and in the app.

## Modules

| Module | Role | Where it runs |
| --- | --- | --- |
| `GitCore` | Models, `ProcessRunner`, parsers, `GitCLIEngine`, `RepositorySession`, recents | Always |
| `AriadnaAI` | Protocol, Copilot, Codex, orchestrator | Only with `ARIADNA_AI=1` |
| `App` | SwiftUI. The executable product is called `Ariadna` | UI macOS only |

## What this foundation includes

Open repo, status, stage/unstage of files and hunks, commit (with a warning if HEAD is detached), log, diff, branches, merge, three-way conflicts, fetch/pull/push. The Phase 1 UI does not show the AI. With `ARIADNA_AI=1` the commit message, summary, explanation and conflict proposal come back.

## What it does not include

Clone, init, amend, interactive rebase, visual cherry-pick, stash, tags, submodules, LFS, worktrees, side-by-side diff, history graph, staging individual lines, Copilot SDK, Codex app-server, notarization, icon, packaging a `.app`.

## How it is checked

- `swift test` on Linux with real temporary repos and simulated CLIs.
- On a Mac: open `Package.swift` in Xcode 16 or later and run the `Ariadna` scheme. The UI could not be run in this delivery's environment (no macOS SDK). The `.github/workflows/ci.yml` workflow builds it on a `macos-15` runner.
