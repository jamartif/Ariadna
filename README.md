# Ariadna

**A desktop Git client for Windows and macOS with an optional AI assistant.**
Built with Rust (Tauri 2) and Svelte 5. It drives the `git` you already have installed, so
hooks, credentials, `credential.helper` and configuration are exactly the same as in your
terminal, with no layer in between that can drift. On top of that it adds a clear interface
for staging changes line by line, browsing history, managing branches, remotes, tags and
stashes, and drafting commit messages, pull request descriptions or conflict resolutions with
the official AI CLIs you already pay for.

Hard rule: **nothing that rewrites your repository happens without your confirmation**, and
the AI **only proposes**.

> **Version 0.1.0.** The previous Swift/SwiftUI app (macOS only) is preserved in the
> [`legacy-swift`](https://github.com/jamartif/Ariadna/tree/legacy-swift) branch and the
> `swift-final` tag (the `v0.1.0` tag in this repository is that **older Swift app**, not
> this one). The latest published build of this migration is
> [`v0.1.0-alpha.3`](https://github.com/jamartif/Ariadna/releases/tag/v0.1.0-alpha.3).
> The specification lives in [`specs/rust/`](specs/rust/) and the task list plus known debt
> in [`specs/rust/tasks.md`](specs/rust/tasks.md).

## Verification status

What has actually been tested, and where. If something has not been run against the real
service, it says "not verified" here:

| Area | Status | Evidence |
|---|---|---|
| Git engine: status, staging per *hunk* and per line, commit, amend, cursor-based log, branches, merge, rebase, cherry-pick, conflicts, remotes, stash, tags, clone with PAT | **Verified** against real `git` | Rust workspace test suite; CI runs it on Linux, macOS and Windows |
| User interface | **Verified** with Vitest and axe in jsdom; visual review documented | Vitest suite in CI; [`specs/rust/qa-alpha2.md`](specs/rust/qa-alpha2.md), [`specs/rust/contraste.md`](specs/rust/contraste.md) |
| End-to-end | **Verified on Linux** with `tauri-driver`: real UI flows plus visual regression with baselines | [`e2e/README.md`](e2e/README.md) |
| **GitHub** pull requests | **VERIFIED against the real API** (2026-11-11) on a disposable private repo: 72 checks OK, 5 limits of GitHub itself, 0 failures | [`specs/rust/spike-pr.md`](specs/rust/spike-pr.md) |
| **Azure DevOps** pull requests | **NOT VERIFIED**: no account or PAT available; tested only against a simulated server (`wiremock`) | [`specs/rust/spike-pr.md`](specs/rust/spike-pr.md) |
| AI with **Claude** | **Verified** with `claude 2.1.319` (flags, `claude auth status --json`, `CLAUDE_CONFIG_DIR`) | [`specs/rust/checklist-humo-ia.md`](specs/rust/checklist-humo-ia.md) |
| AI with **Codex** and **Copilot** | **NOT TESTED** with the real CLI; flags and output formats are still experimental | [`specs/rust/tasks.md`](specs/rust/tasks.md) §M6 |
| Installers | They build (NSIS x64 and universal `.dmg`) but are **unsigned** | [Installing without a signature](docs/instalacion-sin-firma.md) (Spanish) |

## What it does

- **One tab per repository**, with the session restored on start, deduplication by real path
  (linked *worktrees* share a tab) and hibernation of hidden tabs.
- **Changes**: diff per *hunk* and **per single line** (checkboxes, click, Shift+click and
  keyboard), stage/unstage by block or by line, and a commit bar with `Ctrl/⌘+Enter`.
- **Stash**: save (with or without untracked files), list, view the diff, apply, pop and drop
  with confirmation; conflicts on apply land in the Conflicts panel.
- **History** paginated **by cursor** (constant cost even on 100,000-commit repos), with a
  graph, local filter, global search (message, author, hash) and per-commit diff.
- **Branches** in a persistent rail on the left, always visible: switch, create, rename,
  delete (second confirmation when unmerged) and merge, with upstream and ahead/behind
  counters; remote checkout warns when it would diverge.
- **Remotes**: list with the provider recognised, add, rename, remove, fetch, pull and push
  the current branch, plus "Open in browser" / "Create pull request".
- **Conflicts** in a three-way panel (ours/base/theirs) with an editable result; mark as
  resolved, continue or abort merge, rebase and cherry-pick. Optional **"Resolve with AI"**
  proposes a full file without conflict markers, which you review before using.
- **Tags**: list (lightweight and annotated), create on HEAD or on a commit, delete locally
  and publish/delete on the remote.
- **Clone from a URL** (GitHub, Azure DevOps, GitLab, Bitbucket or generic) with an
  **optional PAT**, used once and forgotten, never stored (see *Security*).
- **Pull requests** for GitHub and Azure DevOps: list, detail with safe Markdown, create,
  check out the branch, review with inline comments, vote and complete/abandon.
- **Settings** (General, Git, AI, Accounts), light/dark/system theme, command palette, native
  menu and shortcuts. Interface available in **Spanish and English**.

Out of scope for this version: `init`, submodules, LFS, *worktrees* beyond detecting them,
interactive rebase, a visual cherry-pick, a standalone commit graph view, side-by-side diff,
auto-updater and app stores.

## Installation

Requirements: **Windows 10 (22H2) or 11, x64** · **macOS 15 or later** · `git` **2.39 or
later** on `PATH`. On Windows the installer checks for WebView2 and installs it if missing;
no administrator rights needed.

The 0.1.0 installers are **unsigned**, so the system will warn or block depending on the
machine's configuration. Download them from the *releases* page (current alpha:
`Ariadna_0.1.0_x64-setup.exe` for Windows and `Ariadna_0.1.0_universal.dmg` for macOS, with
`SHA256SUMS.txt` alongside). Step-by-step guide, what changes once there is a signature and
what we **do not** promise: **[docs/instalacion-sin-firma.md](docs/instalacion-sin-firma.md)**
(Spanish).

Summary:

- **Windows** — SmartScreen shows "Windows protected your PC": *More info* → *Run anyway*. If
  the machine has **Smart App Control** enabled, the installer **cannot run** (not even with
  "Run anyway"); the only real fixes are a code signature or turning Smart App Control off,
  which is the user's or their company's decision, not Ariadna's. There is a **portable ZIP**
  and a guide to the options that do work (Windows Sandbox, another machine):
  [docs/windows-sin-instalar.md](docs/windows-sin-instalar.md) (Spanish).
- **macOS** — Gatekeeper will say it cannot verify the app. On macOS 15 right-click → *Open*
  is **no longer enough**: go to *System Settings → Privacy & Security → Open Anyway* (or
  `xattr -dr com.apple.quarantine /Applications/Ariadna.app`). Company-managed Macs may block
  it by policy.

## AI (optional, off until you enable it)

Ariadna uses **no API keys and no third-party proxies**: it launches the **official CLIs**
with the session you already have on the machine, and the prompt travels over **stdin**
(never through `argv`).

| Provider | CLI | Status |
|---|---|---|
| Claude (Pro/Max/Ultra or Team/Enterprise) | `claude` | Verified with `claude 2.1.319` |
| Codex (ChatGPT Free/Go/Plus/Pro/Business/Enterprise) | `codex` | Experimental, untested |
| GitHub Copilot (any plan) | `copilot` | Experimental, untested |

- **Auto mode**: Claude → Codex → Copilot, skipping only the ones not installed and only
  among the providers you have consented to.
- **Personal and Company profiles**: each repo is bound to a profile; the profile decides
  which providers are allowed, with which consent and, when the CLI supports it, with which
  configuration directory (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`). In **Company** the AI is off by
  default and the session is isolated; Copilot **shares the login** between profiles (there is
  no verified way to isolate it) and the app warns about it.
- **Consent** the first time per profile and provider, with a preview of what is sent;
  `.env*`, `*.pem`, `id_*`, `*.key`, `*.pfx` are excluded and secrets are redacted.
- **The AI only proposes**: the result is a proposal you can accept, edit or discard; it never
  writes to the worktree, the index or the refs.

**Before using AI with company code**, check your account terms and internal rules: individual
plans (Claude Pro, ChatGPT Plus, Copilot Pro) may use your inputs for training unless you opt
out, while organisation plans exclude it by contract. Summary with dates and links:
**[specs/rust/terminos-ia.md](specs/rust/terminos-ia.md)** (Spanish).

## Security

- **Tokens**: Ariadna does not store, cache or log PATs or tokens. To clone, the secret is
  handed to `git` **once** through a temporary `GIT_ASKPASS`, used in memory (`zeroize`) and
  destroyed; for pull requests it is requested from `git credential fill` on every call. It
  never appears in `argv`, a URL, `Debug` output or an error message.
- **Processes**: `git` and the CLIs are spawned without a shell, with an absolute path, an
  allow-listed environment (`GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`, no `*_API_KEY`),
  `fsmonitor` and external hooks disabled for reads, and a process tree that is terminated on
  close (Job Object on Windows, process group on macOS).
- **Hostile repositories**: validated paths (`../`, UNC, `\\?\`, reserved names, `.git` with
  NTFS variants, `-rf`, magic pathspecs), refs checked with `git check-ref-format` and patches
  built from `git diff` bytes.
- **Network**: the only traffic of our own is the repository provider's API for pull requests,
  only on your action or with the panel open; TLS always verified, no telemetry. It honours
  the **proxy** (`HTTPS_PROXY`/`HTTP_PROXY`/`NO_PROXY`) and the system **certificates**
  (`SSL_CERT_FILE`, `SSL_CERT_DIR`), including those a company installs, implemented and
  tested in `crates/ariadna-hosting/src/net.rs`.
- **Public repo**: `gitleaks` scans every commit (`.githooks/pre-commit`, installable with
  `sh scripts/install-git-hooks.sh`) and CI runs it over the whole history.

## Architecture

```
Svelte 5 UI ──typed IPC──▶ Tauri commands ──▶ AppCore ──▶ RepositorySession
                                                  │              │
                                                  │              ▼
                                                  │        GitEngine ──▶ ProcessRunner ──▶ git
                                                  ├──▶ AiService ──▶ AiProvider ──▶ claude | codex | copilot
                                                  └──▶ ariadna-hosting ──▶ PullRequestProvider ──▶ GitHub / Azure DevOps REST
```

State lives in Rust; the interface is a view with no git or AI logic. Each piece of the
workspace:

| Crate | Contents |
|---|---|
| `crates/ariadna-process` | Process launcher: no shell, cooperative cancellation, timeout, byte cap, Job Object / process group |
| `crates/ariadna-git` | Git engine and parsers (`status --porcelain=v2 -z`, diff, log, branches, remotes, diff3 conflicts); per-repo serialised session, stash, line-level patches (`partial`), clone, credentials and *askpass* |
| `crates/ariadna-ai` | AI providers via the official CLI, allow-listed environment, prompts, secret redaction, profiles and queue |
| `crates/ariadna-hosting` | Provider recognition (`RemoteInfo`), HTTP client (`reqwest` + `rustls`) with system proxy and CA (`net.rs`), GitHub and Azure DevOps pull requests |
| `src-tauri/` | Tauri application: windowless testable core, IPC commands, file watcher, menu, settings and `session.json` |
| `ui/` | Svelte 5 + TypeScript interface (bilingual Spanish/English) |

## Development

Requirements: stable Rust (`rust-toolchain.toml`), Node 22 and the Tauri 2 system
dependencies.

```sh
# Rust: format, lint and tests
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast

# Interface
cd ui
npm ci
npm run check && npm test && npm run build
npm run dev          # Vite on 127.0.0.1:5173 (simulated backend when opened in a browser)
```

On Windows it is safer to build from **WSL** (`cargo` over `/mnt/c`): some machines block
freshly compiled build scripts with Smart App Control and `cargo build` fails with "An
Application Control policy has blocked this file" (error 4551). The dev profile already
builds those scripts with light optimisation to reduce it, but WSL is the reliable path.

End-to-end and visual tests (Tauri driver, Linux):

```sh
cd e2e && npm ci
xvfb-run -a -s "-screen 0 1440x900x24" npm test        # UI flows against the packaged app
xvfb-run -a -s "-screen 0 1440x900x24" npm run visual  # visual regression
```

Performance tests (`#[ignore]`, synthetic repos with 50,000 files and 100,000 commits):

```sh
cargo test -p ariadna-git --test perf -- --ignored --nocapture
```

## Documentation

> Detailed documents are currently written in Spanish.

| Document | Contents |
|---|---|
| [`docs/instalacion-sin-firma.md`](docs/instalacion-sin-firma.md) | Opening unsigned alphas on Windows (SmartScreen, Smart App Control) and macOS (Gatekeeper) |
| [`docs/windows-sin-instalar.md`](docs/windows-sin-instalar.md) | Testing on Windows without installing (Windows Sandbox, portable ZIP) |
| [`specs/rust/spec.md`](specs/rust/spec.md) | Migration specification (requirements G-xx, PR-xx, RF-IAx, NF-x) |
| [`specs/rust/revision-speckfish.md`](specs/rust/revision-speckfish.md) | Formal spec review: blockers, majors and decisions taken |
| [`specs/rust/tasks.md`](specs/rust/tasks.md) | Milestones, tasks and known debt |
| [`specs/rust/matriz-trazabilidad.md`](specs/rust/matriz-trazabilidad.md) | Requirement → task → real test, gaps marked |
| [`specs/rust/terminos-ia.md`](specs/rust/terminos-ia.md) | Anthropic, OpenAI and GitHub Copilot terms: individual vs organisation |
| [`specs/rust/spike-pr.md`](specs/rust/spike-pr.md) | What is verified about pull requests and what is not |
| [`specs/rust/checklist-humo-ia.md`](specs/rust/checklist-humo-ia.md) | Manual smoke checklist with real AI CLIs |
| [`specs/rust/ipc.md`](specs/rust/ipc.md) | IPC contract between Rust and the interface |

## Code signing policy

> **Free code signing provided by [SignPath.io](https://signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).**

- **Signed artifacts:** the Windows NSIS installer and its executable, built only in GitHub
  Actions from this repository.
- **Committers and reviewers:** [@jamartif](https://github.com/jamartif).
- **Signing approvers:** [@jamartif](https://github.com/jamartif); every release is approved
  by hand.
- **Privacy:** *This program will not transfer any information to other networked systems
  unless specifically requested by the user or the person installing or operating it.* No
  telemetry; the network is used only when you ask for it (git operations against your
  remotes and, if you enable AI, the official CLI you choose).
- **macOS** is not covered by this policy: the `.dmg` is ad-hoc signed until there is a
  Developer ID certificate.

Full details in [`docs/firma-de-codigo.md`](docs/firma-de-codigo.md) (Spanish).

## License

[MIT](LICENSE). Use it, modify it and redistribute it; if you publish a modified version,
keep the copyright notice.
