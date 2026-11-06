# Tasks

Status after aligning Phase 1 with the PM. `[x]` means done in the code. The core tests were run on Linux. The SwiftUI app has not been run here.

## Phase 0 — Spec

- [x] `specs/spec.md`, `specs/plan.md` and this list
- [x] Name **Ariadna**, bundle id `es.javiermartinez.ariadna`
- [x] Minimum macOS 15.0
- [ ] Full github/spec-kit flow (`constitution` / `specify`) if the Spec Kit CLI is wanted on top of these files

## Phase 1 — Git core

- [x] Package: `GitCore`, `GitCoreTests`, `App` (`Ariadna` executable)
- [x] `ProcessRunner`: timeout, cancellation, environment without token-like variables, login shell PATH
- [x] Open repo, with an error if it is not one
- [x] `git status --porcelain=v2 -z`
- [x] Stage and unstage by file
- [x] Stage and unstage a hunk: isolated, contiguous, CRLF, no final newline, path with spaces and Unicode
- [x] Commit. Empty message rejected. Detached HEAD visible and a warning before committing outside a branch
- [x] Log and unified diff
- [x] Branches: list, create, checkout, delete (and rename)
- [x] Fetch, pull merge, pull rebase and push with upstream
- [x] Serial queue for git mutations and cancellable refresh (does not publish state if cancelled)
- [x] Recents and favorites
- [x] Strict `.gitignore` and gitleaks in `.githooks/pre-commit` and in CI
- [x] CI workflow on `macos-15`: `swift build` + `swift test`, and another pass with `ARIADNA_AI=1`
- [ ] Stage individual lines
- [x] Safe amend: refused with no commits, outside a branch, with a merge/rebase in progress or if the commit is already on a remote
- [x] Amend enable/disable race: a late read of the last commit no longer prefills the message (`AmendState`)
- [ ] `git clean` of untracked files (left out on purpose)
- [x] Search across the whole history by message, author or hash (`searchLog`); the UI still filters what is already loaded on the fly
- [x] Git messages forced to English so classification (merge, conflicts, nothing to commit) does not depend on the Mac's language
- [x] The log parser no longer leaves a newline in front of the hash from the second commit on
- [ ] Commit graph
- [ ] Build the app on Javier's Mac (CI tries; this environment has no Apple SDK)
- [ ] English interface

## Merge and conflicts

They stay in the core and are not moved to another phase.

- [x] Merge ff, no-ff and "already up to date"
- [x] ours / base / theirs model
- [x] Markers, including diff3, and choice by hunk without writing to disk
- [x] Write the file only with confirmation
- [x] Mark resolved, continue and abort
- [x] Three-way editor in the UI
- [x] `pull --rebase` test with conflict, abort and continue
- [ ] Cherry-pick conflict test

## Phase 3 — AI (code present, build off)

The `AriadnaAI` target is not compiled unless `ARIADNA_AI=1`.

- [x] Protocol, Copilot CLI, Codex CLI, orchestrator, context cap, proposal sheet
- [x] Process-double tests when the flag is on
- [ ] Try against Javier's real CLIs
- [ ] Copilot SDK, Codex app-server, streaming

## Checks for this delivery

- [x] `swift test` on Linux: 36 `GitCoreTests` tests
- [x] `ARIADNA_AI=1 swift test` on Linux: those 36 plus 8 `AriadnaAITests`
- [ ] Run the app on macOS 15 (done by the CI workflow and, locally, by Javier)
