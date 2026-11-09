import type { Api, Unlisten } from './api';
import {
  APP_SVELTE_ADDED,
  ERROR_DEMO_UNTRACKED,
  IPC_MD_UNSTAGED,
  LEGACY_SWIFT_DELETED,
  MAKEFILE_UNSTAGED,
  MODELS_RS_STAGED,
  NOTAS_UNTRACKED,
  README_RENAMED,
  TYPES_TS_STAGED,
  TYPES_TS_UNSTAGED,
  additionsHunk,
  giantHunk,
  longLineHunk,
  makeCommits,
  sha,
  smallHunk
} from './mockData';
import {
  ApiError,
  type AmendDraft,
  type ApiErrorCode,
  type ApiEvents,
  type AppState,
  type Branch,
  type Commit,
  type ConflictFile,
  type DiffHunk,
  type DiffScope,
  type FileDiff,
  type GitPath,
  type IntegrationState,
  type OpenResult,
  type RecentRepository,
  type Remote,
  type Snapshot,
  type StatusEntry,
  type TabInfo
} from './types';
import { pathText } from '../util/gitpath';

export interface MockOptions {
  /** Latencia simulada por llamada en ms (por defecto 120; 0 en tests). */
  latency?: number;
  /** `empty`, `nogit`, `slow`, `error`. */
  scenario?: string;
  /** Confirmación nativa simulada para `discard`. Por defecto `window.confirm` si existe. */
  confirm?: (message: string) => boolean;
  /** Reloj inyectable. */
  now?: () => Date;
}

type Base = 'M' | 'A' | 'D' | 'R' | '?' | 'U';

interface MockFile {
  path: string;
  originalPath?: string;
  base: Base;
  staged: DiffHunk[];
  unstaged: DiffHunk[];
  binary?: boolean;
  binaryStaged?: boolean;
  ignored?: boolean;
  wasUntracked?: boolean;
  conflictKind?: StatusEntry['conflictKind'];
}

interface MockRepo {
  tabId: string;
  path: string;
  name: string;
  branch: string | null;
  detached: boolean;
  missing: boolean;
  files: MockFile[];
  branches: Branch[];
  remoteBranches: Branch[];
  remotes: Remote[];
  integration: IntegrationState;
  commits: Commit[];
  revision: number;
  busy: string | null;
  ahead: number;
  behind: number;
}

const hasStaged = (f: MockFile) => f.staged.length > 0 || (f.binary === true && f.binaryStaged === true);
const hasUnstaged = (f: MockFile) =>
  f.unstaged.length > 0 || (f.binary === true && f.binaryStaged !== true && f.base !== '?');

function clone<T>(v: T): T {
  return structuredClone(v);
}

function err(code: ApiErrorCode, message: string, detail?: string): ApiError {
  return new ApiError({ code, message, detail });
}

function branch(name: string, i: number, isHead: boolean, upstream: string | null, ahead = 0, behind = 0): Branch {
  return { name, oid: sha(i + 100), isHead, upstream, ahead, behind };
}

function buildAriadna(): MockRepo {
  const files: MockFile[] = [
    { path: 'src/lib/api/types.ts', base: 'M', staged: clone(TYPES_TS_STAGED), unstaged: clone(TYPES_TS_UNSTAGED) },
    { path: 'crates/ariadna-git/src/models.rs', base: 'M', staged: clone(MODELS_RS_STAGED), unstaged: [] },
    { path: 'ui/src/App.svelte', base: 'A', staged: [clone(APP_SVELTE_ADDED)], unstaged: [] },
    { path: 'docs/ipc.md', base: 'M', staged: [], unstaged: clone(IPC_MD_UNSTAGED) },
    { path: 'Makefile', base: 'M', staged: [], unstaged: clone(MAKEFILE_UNSTAGED) },
    { path: 'Sources/App/Legacy.swift', base: 'D', staged: [], unstaged: [clone(LEGACY_SWIFT_DELETED)] },
    { path: 'docs/README.md', originalPath: 'README.md', base: 'R', staged: clone(README_RENAMED), unstaged: [] },
    { path: 'docs/screenshots/staging.png', base: 'M', staged: [], unstaged: [], binary: true },
    { path: 'web/minified.js', base: 'M', staged: [], unstaged: [longLineHunk()] },
    { path: 'scratch/big-fixture.json', base: 'M', staged: [], unstaged: [giantHunk()] },
    { path: 'notas.txt', base: '?', staged: [], unstaged: [clone(NOTAS_UNTRACKED)], wasUntracked: true },
    { path: 'docs/error-demo.md', base: '?', staged: [], unstaged: [clone(ERROR_DEMO_UNTRACKED)], wasUntracked: true },
    { path: '.env.local', base: '?', staged: [], unstaged: [], ignored: true },
    { path: 'target/', base: '?', staged: [], unstaged: [], ignored: true }
  ];
  return {
    tabId: 'tab-ariadna',
    path: '/home/javie/proyectos/ariadna',
    name: 'ariadna',
    branch: 'rust-migration',
    detached: false,
    missing: false,
    files,
    branches: [
      branch('rust-migration', 1, true, 'origin/rust-migration', 3, 0),
      branch('main', 2, false, 'origin/main', 0, 2),
      branch('feature/command-palette', 3, false, null)
    ],
    remoteBranches: [
      branch('origin/rust-migration', 4, false, null),
      branch('origin/main', 5, false, null)
    ],
    remotes: [
      {
        name: 'origin',
        fetchUrl: 'git@github.com:javie/ariadna.git',
        pushUrl: 'git@github.com:javie/ariadna.git'
      }
    ],
    integration: 'none',
    commits: makeCommits([
      'Añade el motor de diff por hunk',
      'Implementa recientes con favoritos',
      'Refactor del runner de git con tope de salida',
      'Corrige el parseo de renombrados en status v2',
      'Documenta el contrato IPC',
      'Primer esqueleto del workspace de Cargo'
    ]),
    revision: 1,
    busy: null,
    ahead: 3,
    behind: 0
  };
}

function buildConflicts(): MockRepo {
  const conflicted = (p: string, kind: MockFile['conflictKind']): MockFile => ({
    path: p,
    base: 'U',
    staged: [],
    unstaged: [],
    conflictKind: kind
  });
  return {
    tabId: 'tab-legacy',
    path: '/home/javie/proyectos/legacy-api',
    name: 'legacy-api',
    branch: 'feature/payments',
    detached: false,
    missing: false,
    files: [
      conflicted('src/billing/invoice.ts', 'bothModified'),
      conflicted('src/billing/tax.ts', 'deletedByThem'),
      { path: 'package.json', base: 'M', staged: [smallHunk('package.json', 3)], unstaged: [] },
      { path: 'README.md', base: 'M', staged: [], unstaged: [smallHunk('README.md', 1)] }
    ],
    branches: [
      branch('feature/payments', 11, true, 'origin/feature/payments', 1, 4),
      branch('main', 12, false, 'origin/main')
    ],
    remoteBranches: [branch('origin/main', 13, false, null), branch('origin/feature/payments', 14, false, null)],
    remotes: [{ name: 'origin', fetchUrl: 'https://git.example.com/legacy-api.git', pushUrl: 'https://git.example.com/legacy-api.git' }],
    integration: 'merging',
    commits: makeCommits(['Integra pasarela de pagos', 'Añade modelo de factura', 'Configura CI']),
    revision: 1,
    busy: null,
    ahead: 1,
    behind: 4
  };
}

function buildGigante(): MockRepo {
  const files: MockFile[] = Array.from({ length: 3000 }, (_, i) => {
    const group = `packages/pkg-${String(i % 60).padStart(2, '0')}/src`;
    const p = `${group}/module-${String(i).padStart(4, '0')}.rs`;
    return { path: p, base: 'M' as Base, staged: [], unstaged: [smallHunk(p, i)] };
  });
  return {
    tabId: 'tab-gigante',
    path: '/home/javie/proyectos/monorepo-gigante',
    name: 'monorepo-gigante',
    branch: 'main',
    detached: false,
    missing: false,
    files,
    branches: [branch('main', 21, true, 'origin/main')],
    remoteBranches: [branch('origin/main', 22, false, null)],
    remotes: [{ name: 'origin', fetchUrl: 'git@example.com:mono.git', pushUrl: 'git@example.com:mono.git' }],
    integration: 'none',
    commits: makeCommits(['Actualiza dependencias', 'Mueve utilidades a pkg-00']),
    revision: 1,
    busy: null,
    ahead: 0,
    behind: 0
  };
}

function buildMissing(): MockRepo {
  return {
    tabId: 'tab-antiguo',
    path: '/home/javie/proyectos/antiguo',
    name: 'antiguo',
    branch: null,
    detached: false,
    missing: true,
    files: [],
    branches: [],
    remoteBranches: [],
    remotes: [],
    integration: 'none',
    commits: [],
    revision: 1,
    busy: null,
    ahead: 0,
    behind: 0
  };
}

function buildSmall(path: string, n: number): MockRepo {
  const name = path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || 'repo';
  return {
    tabId: `tab-new-${n}`,
    path,
    name,
    branch: 'main',
    detached: false,
    missing: false,
    files: [
      { path: 'README.md', base: 'M', staged: [], unstaged: [smallHunk('README.md', 1)] },
      { path: 'src/main.rs', base: '?', staged: [], unstaged: [additionsHunk('main.rs#u1', ['fn main() {}'])], wasUntracked: true }
    ],
    branches: [branch('main', 30 + n, true, null)],
    remoteBranches: [],
    remotes: [],
    integration: 'none',
    commits: makeCommits(['Commit inicial']),
    revision: 1,
    busy: null,
    ahead: 0,
    behind: 0
  };
}

const BASE_RECENTS: RecentRepository[] = [
  { name: 'ariadna', path: '/home/javie/proyectos/ariadna', isFavorite: true, lastBranch: 'rust-migration', lastOpened: '2026-11-09T09:12:00+01:00' },
  { name: 'legacy-api', path: '/home/javie/proyectos/legacy-api', isFavorite: true, lastBranch: 'feature/payments', lastOpened: '2026-11-08T17:40:00+01:00' },
  { name: 'monorepo-gigante', path: '/home/javie/proyectos/monorepo-gigante', isFavorite: false, lastBranch: 'main', lastOpened: '2026-11-07T11:05:00+01:00' },
  { name: 'dotfiles', path: '/home/javie/dotfiles', isFavorite: false, lastBranch: 'main', lastOpened: '2026-10-30T20:15:00+01:00' },
  { name: 'apuntes-rust', path: '/home/javie/estudio/apuntes-rust', isFavorite: false, lastBranch: 'master', lastOpened: '2026-09-02T08:00:00+02:00' },
  { name: 'antiguo', path: '/home/javie/proyectos/antiguo', isFavorite: false, lastBranch: 'main', lastOpened: '2026-06-12T08:00:00+02:00' }
];

/**
 * Backend simulado en memoria con la misma interfaz que el real. Los cambios de
 * estado (stage, unstage, commit…) modifican el modelo y emiten `snapshot` con
 * una `revision` creciente, igual que Rust.
 */
export function createMockApi(options: MockOptions = {}): Api & { __repos: Map<string, MockRepo> } {
  const scenario = options.scenario;
  const latency = (options.latency ?? 120) * (scenario === 'slow' ? 6 : 1);
  const now = options.now ?? (() => new Date());
  const confirmFn =
    options.confirm ??
    ((msg: string) => (typeof window !== 'undefined' && typeof window.confirm === 'function' ? window.confirm(msg) : true));

  const repos = new Map<string, MockRepo>();
  let order: string[] = [];
  let activeTabId: string | null = null;
  let recents: RecentRepository[] = [];
  let newCounter = 1;
  let opCounter = 0;
  const listeners = new Map<string, Set<(p: never) => void>>();

  if (scenario !== 'empty' && scenario !== 'nogit' && scenario !== 'error') {
    for (const r of [buildAriadna(), buildConflicts(), buildMissing()]) {
      repos.set(r.tabId, r);
      order.push(r.tabId);
    }
    activeTabId = 'tab-ariadna';
  }
  if (scenario !== 'empty') recents = clone(BASE_RECENTS);

  const delay = (ms = latency) => (ms <= 0 ? Promise.resolve() : new Promise<void>((r) => setTimeout(r, ms)));

  function emit<K extends keyof ApiEvents>(event: K, payload: ApiEvents[K]): void {
    const set = listeners.get(event);
    if (!set) return;
    for (const h of [...set]) (h as (p: ApiEvents[K]) => void)(clone(payload));
  }

  function repoOf(tabId: string): MockRepo {
    const r = repos.get(tabId);
    if (!r) throw err('REPO_MISSING', 'La pestaña ya no existe.');
    if (r.missing) throw err('REPO_MISSING', `No se encuentra la carpeta del repositorio: ${r.path}`);
    return r;
  }

  function entriesOf(r: MockRepo): StatusEntry[] {
    const out: StatusEntry[] = [];
    for (const f of r.files) {
      const base = {
        path: f.path as GitPath,
        originalPath: (f.originalPath ?? null) as GitPath | null,
        isSubmodule: false
      };
      if (f.base === 'U') {
        out.push({ ...base, indexStatus: 'U', workTreeStatus: 'U', isConflicted: true, isUntracked: false, isIgnored: false, conflictKind: f.conflictKind ?? 'bothModified' });
      } else if (f.ignored) {
        out.push({ ...base, indexStatus: '.', workTreeStatus: '!', isConflicted: false, isUntracked: false, isIgnored: true, conflictKind: null });
      } else if (f.base === '?' && !hasStaged(f)) {
        out.push({ ...base, indexStatus: '?', workTreeStatus: '?', isConflicted: false, isUntracked: true, isIgnored: false, conflictKind: null });
      } else {
        const idx = hasStaged(f) ? (f.base === '?' ? 'A' : f.base) : '.';
        const wt = hasUnstaged(f) ? (f.base === 'D' ? 'D' : f.base === 'R' || f.base === '?' || f.base === 'A' ? 'M' : f.base) : '.';
        out.push({ ...base, indexStatus: idx, workTreeStatus: wt, isConflicted: false, isUntracked: false, isIgnored: false, conflictKind: null });
      }
    }
    return out;
  }

  function changeCount(r: MockRepo): number {
    return entriesOf(r).filter((e) => !e.isIgnored).length;
  }

  function tabInfo(r: MockRepo): TabInfo {
    const entries = entriesOf(r);
    return {
      tabId: r.tabId,
      path: r.path,
      name: r.name,
      branch: r.branch,
      detached: r.detached,
      changeCount: entries.filter((e) => !e.isIgnored).length,
      conflictCount: entries.filter((e) => e.isConflicted).length,
      busy: r.busy,
      missing: r.missing
    };
  }

  function conflictsOf(r: MockRepo): ConflictFile[] {
    return r.files
      .filter((f) => f.base === 'U')
      .map((f) => ({
        path: f.path,
        kind: f.conflictKind ?? 'bothModified',
        ours: { oid: sha(1), content: 'ours\n', isBinary: false },
        base: { oid: sha(2), content: 'base\n', isBinary: false },
        theirs: { oid: sha(3), content: 'theirs\n', isBinary: false },
        workingTree: '<<<<<<< ours\nours\n=======\ntheirs\n>>>>>>> theirs\n',
        hunks: [{ id: 0, ours: 'ours\n', base: 'base\n', theirs: 'theirs\n' }]
      }));
  }

  function snapshotOf(r: MockRepo): Snapshot {
    return {
      revision: r.revision,
      status: {
        branchName: r.branch,
        headOid: r.commits[0]?.sha ?? null,
        isDetached: r.detached,
        isUnborn: r.commits.length === 0,
        upstream: r.branches.find((b) => b.isHead)?.upstream ?? null,
        ahead: r.ahead,
        behind: r.behind,
        entries: entriesOf(r)
      },
      branches: r.branches,
      remoteBranches: r.remoteBranches,
      remotes: r.remotes,
      integration: r.integration,
      conflicts: conflictsOf(r)
    };
  }

  /** Publica un nuevo snapshot (revisión + 1) tras una mutación, como el refresco programado de Rust. */
  function publish(r: MockRepo): void {
    r.revision += 1;
    const rev = r.revision;
    setTimeout(() => {
      if (!repos.has(r.tabId) || r.revision !== rev) return;
      emit('snapshot', { tabId: r.tabId, snapshot: snapshotOf(r) });
      emit('tab-updated', tabInfo(r));
    }, Math.min(latency / 2, 80));
  }

  function appState(): AppState {
    return {
      tabs: order.map((id) => tabInfo(repos.get(id) as MockRepo)),
      activeTabId,
      recents: clone(recents),
      git:
        scenario === 'nogit'
          ? null
          : { path: '/usr/bin/git', version: '2.47.0' },
      gitProblem:
        scenario === 'nogit'
          ? 'No se encontró Git en el equipo. Ariadna necesita Git 2.39 o posterior.'
          : null
    };
  }

  function touchRecent(r: MockRepo): void {
    const prev = recents.find((x) => x.path === r.path);
    const entry: RecentRepository = {
      name: r.name,
      path: r.path,
      isFavorite: prev?.isFavorite ?? false,
      lastBranch: r.branch,
      lastOpened: now().toISOString()
    };
    recents = [entry, ...recents.filter((x) => x.path !== r.path)];
  }

  function sortedRecents(): RecentRepository[] {
    return clone(recents);
  }

  async function operation<T>(r: MockRepo, label: string, fn: () => T | Promise<T>): Promise<T> {
    const operationId = ++opCounter;
    emit('session-event', { sessionId: r.tabId, revision: r.revision, kind: { type: 'operationStarted', operationId, label } });
    r.busy = label;
    emit('tab-updated', tabInfo(r));
    try {
      await delay();
      const out = await fn();
      return out;
    } catch (e) {
      const apiErr = e instanceof ApiError ? e : err('COMMAND_FAILED', String(e));
      emit('session-event', {
        sessionId: r.tabId,
        revision: r.revision,
        kind: {
          type: 'operationFinished',
          operationId,
          label,
          ok: false,
          error: { code: apiErr.code, message: apiErr.message, detail: apiErr.detail }
        }
      });
      r.busy = null;
      emit('tab-updated', tabInfo(r));
      throw e;
    } finally {
      if (r.busy === label) {
        r.busy = null;
        emit('session-event', {
          sessionId: r.tabId,
          revision: r.revision,
          kind: { type: 'operationFinished', operationId, label, ok: true, error: null }
        });
        emit('tab-updated', tabInfo(r));
      }
    }
  }

  function findFile(r: MockRepo, p: GitPath): MockFile {
    const f = r.files.find((x) => x.path === pathText(p));
    if (!f) throw err('PATH_ESCAPES_REPOSITORY', `La ruta ${pathText(p)} no pertenece al repositorio.`);
    return f;
  }

  function sortHunks(h: DiffHunk[]): DiffHunk[] {
    return h.sort((a, b) => a.oldStart - b.oldStart || a.newStart - b.newStart);
  }

  /** Falla antes de tocar nada, como git (la operación es atómica). */
  function checkStageable(f: MockFile): void {
    if (f.path === 'docs/error-demo.md') {
      throw err(
        'LOCKED',
        'Otro proceso de git está usando el repositorio. Inténtalo de nuevo en unos segundos.',
        "fatal: Unable to create '/home/javie/proyectos/ariadna/.git/index.lock': File exists.\n\nAnother git process seems to be running in this repository."
      );
    }
  }

  function stageFile(f: MockFile): void {
    if (f.binary) f.binaryStaged = true;
    f.staged = sortHunks([...f.staged, ...f.unstaged]);
    f.unstaged = [];
    if (f.base === '?') f.base = 'A';
  }

  function unstageFile(f: MockFile): void {
    if (f.binary) f.binaryStaged = false;
    f.unstaged = sortHunks([...f.unstaged, ...f.staged]);
    f.staged = [];
    if (f.base === 'A' && f.wasUntracked) f.base = '?';
    if (f.base === 'R') f.base = 'M';
  }

  function moveHunk(from: DiffHunk[], to: DiffHunk[], hunk: DiffHunk): void {
    const i = from.findIndex(
      (h) =>
        h.id === hunk.id ||
        (h.oldStart === hunk.oldStart && h.oldCount === hunk.oldCount && h.newStart === hunk.newStart && h.newCount === hunk.newCount)
    );
    if (i < 0) throw err('HUNK_NOT_FOUND', 'El hunk ya no existe: el archivo cambió. Se ha actualizado la vista.');
    const [h] = from.splice(i, 1);
    to.push(h as DiffHunk);
    sortHunks(to);
  }

  const api: Api & { __repos: Map<string, MockRepo> } = {
    __repos: repos,

    async appState() {
      await delay();
      if (scenario === 'error') throw err('IO', 'No se pudo leer el estado de la aplicación.', 'Error: session.json: permiso denegado (os error 13)');
      return appState();
    },
    async pickFolder() {
      await delay(Math.min(latency, 200));
      return '/home/javie/proyectos/demo-nuevo';
    },
    async openRepository(path): Promise<OpenResult> {
      await delay();
      const clean = path.trim().replace(/[\\/]+$/, '');
      if (clean === '') throw err('NOT_A_REPOSITORY', 'Indica la ruta de una carpeta.');
      if (/no-repo/i.test(clean)) {
        throw err('NOT_A_REPOSITORY', `La carpeta ${clean} no es un repositorio de Git.`, `fatal: not a git repository (or any of the parent directories): .git`);
      }
      if (/missing/i.test(clean)) throw err('REPO_MISSING', `No se encuentra la carpeta ${clean}.`);
      if (scenario === 'nogit') throw err('GIT_NOT_FOUND', 'No se encontró Git en el equipo.');
      const existing = [...repos.values()].find((r) => r.path === clean);
      if (existing) {
        activeTabId = existing.tabId;
        touchRecent(existing);
        emit('tabs-changed', appState());
        return { tab: tabInfo(existing), alreadyOpen: true };
      }
      let repo: MockRepo;
      if (clean === '/home/javie/proyectos/monorepo-gigante') repo = buildGigante();
      else repo = buildSmall(clean, newCounter++);
      repos.set(repo.tabId, repo);
      order.push(repo.tabId);
      activeTabId = repo.tabId;
      touchRecent(repo);
      emit('tabs-changed', appState());
      return { tab: tabInfo(repo), alreadyOpen: false };
    },
    async closeTab(tabId) {
      await delay(Math.min(latency, 60));
      if (!repos.has(tabId)) return { closed: false };
      const i = order.indexOf(tabId);
      repos.delete(tabId);
      order = order.filter((x) => x !== tabId);
      if (activeTabId === tabId) activeTabId = order[Math.min(i, order.length - 1)] ?? null;
      emit('tabs-changed', appState());
      return { closed: true };
    },
    async activateTab(tabId) {
      if (repos.has(tabId)) activeTabId = tabId;
    },
    async reorderTabs(tabIds) {
      const known = tabIds.filter((x) => repos.has(x));
      const rest = order.filter((x) => !known.includes(x));
      order = [...known, ...rest];
    },
    async toggleFavorite(path) {
      await delay(Math.min(latency, 60));
      recents = recents.map((r) => (r.path === path ? { ...r, isFavorite: !r.isFavorite } : r));
      return sortedRecents();
    },
    async forgetRecent(path) {
      await delay(Math.min(latency, 60));
      recents = recents.filter((r) => r.path !== path);
      return sortedRecents();
    },
    async cancelOperation() {
      return false;
    },

    async getSnapshot(tabId) {
      await delay();
      return clone(snapshotOf(repoOf(tabId)));
    },
    async refresh(tabId) {
      await delay();
      const r = repoOf(tabId);
      r.revision += 1;
      return clone(snapshotOf(r));
    },
    async stage(tabId, paths) {
      const r = repoOf(tabId);
      await operation(r, paths.length === 1 ? `Preparando ${pathText(paths[0] as GitPath)}` : `Preparando ${paths.length} archivos`, () => {
        const files = paths.map((p) => findFile(r, p));
        files.forEach(checkStageable);
        files.forEach(stageFile);
        publish(r);
      });
    },
    async unstage(tabId, paths) {
      const r = repoOf(tabId);
      await operation(r, paths.length === 1 ? `Quitando ${pathText(paths[0] as GitPath)}` : `Quitando ${paths.length} archivos`, () => {
        paths.map((p) => findFile(r, p)).forEach(unstageFile);
        publish(r);
      });
    },
    async stageHunk(tabId, path, hunk) {
      const r = repoOf(tabId);
      await operation(r, 'Preparando hunk', () => {
        const f = findFile(r, path);
        moveHunk(f.unstaged, f.staged, hunk);
        if (f.base === '?') f.base = 'A';
        publish(r);
      });
    },
    async unstageHunk(tabId, path, hunk) {
      const r = repoOf(tabId);
      await operation(r, 'Quitando hunk', () => {
        const f = findFile(r, path);
        moveHunk(f.staged, f.unstaged, hunk);
        if (f.staged.length === 0 && f.base === 'A' && f.wasUntracked) f.base = '?';
        publish(r);
      });
    },
    async discard(tabId, paths) {
      const r = repoOf(tabId);
      return operation(r, 'Descartando cambios', () => {
        const names = paths.map((p) => pathText(p)).join(', ');
        if (!confirmFn(`¿Descartar los cambios de ${names} en ${r.name} (${r.branch ?? 'HEAD'})? No se puede deshacer.`)) {
          return { done: false };
        }
        for (const p of paths) {
          const f = findFile(r, p);
          if (f.base === '?') r.files = r.files.filter((x) => x !== f);
          else {
            f.unstaged = [];
            if (f.binary && !f.binaryStaged) f.binary = false;
            if (!hasStaged(f)) r.files = r.files.filter((x) => x !== f);
          }
        }
        publish(r);
        return { done: true };
      });
    },
    async commit(tabId, message) {
      const r = repoOf(tabId);
      return operation(r, 'Creando commit', () => {
        if (message.trim() === '') throw err('EMPTY_COMMIT_MESSAGE', 'El mensaje del commit no puede estar vacío.');
        const staged = r.files.filter(hasStaged);
        if (staged.length === 0) throw err('NOTHING_TO_COMMIT', 'No hay nada preparado para confirmar.');
        const lines = message.trim().split('\n');
        const commit: Commit = {
          sha: sha(r.commits.length + 50 + r.revision),
          parents: r.commits[0] ? [r.commits[0].sha] : [],
          subject: lines[0] ?? '',
          body: lines.slice(1).join('\n').trim(),
          authorName: 'Javier',
          authorEmail: 'javier@example.com',
          authorDateIso: now().toISOString(),
          refs: [`HEAD -> ${r.branch ?? ''}`]
        };
        r.commits = [commit, ...r.commits];
        for (const f of staged) {
          f.staged = [];
          f.binaryStaged = false;
          if (f.unstaged.length === 0) r.files = r.files.filter((x) => x !== f);
          else f.base = 'M';
        }
        r.ahead += 1;
        touchRecent(r);
        publish(r);
        return { done: true, sha: commit.sha };
      });
    },
    async amend(tabId, message) {
      const r = repoOf(tabId);
      return operation(r, 'Corrigiendo el último commit', () => {
        if (message.trim() === '') throw err('EMPTY_COMMIT_MESSAGE', 'El mensaje del commit no puede estar vacío.');
        const last = r.commits[0];
        if (!last) throw err('CANNOT_AMEND', 'No hay ningún commit que corregir.');
        const lines = message.trim().split('\n');
        r.commits[0] = { ...last, subject: lines[0] ?? '', body: lines.slice(1).join('\n').trim() };
        for (const f of r.files.filter(hasStaged)) {
          f.staged = [];
          f.binaryStaged = false;
          if (f.unstaged.length === 0) r.files = r.files.filter((x) => x !== f);
          else f.base = 'M';
        }
        publish(r);
        return last.sha;
      });
    },
    async amendDraft(tabId): Promise<AmendDraft> {
      await delay(Math.min(latency, 60));
      const r = repoOf(tabId);
      const last = r.commits[0];
      if (!last) return { message: '', canAmend: false, reason: 'Todavía no hay ningún commit que corregir.' };
      if (r.integration !== 'none') {
        return { message: '', canAmend: false, reason: 'No se puede corregir mientras hay una integración en curso.' };
      }
      return { message: last.body ? `${last.subject}\n\n${last.body}` : last.subject, canAmend: true, reason: null };
    },
    async diff(tabId, scope: DiffScope, paths): Promise<FileDiff[]> {
      await delay(Math.min(latency, 80));
      const r = repoOf(tabId);
      if (typeof scope !== 'string') return [];
      return paths.map((p) => {
        const f = findFile(r, p);
        const hunks = scope === 'staged' ? f.staged : f.unstaged;
        return clone({
          path: f.path,
          oldPath: f.originalPath ?? null,
          isBinary: f.binary === true,
          isNewFile: f.base === 'A' || f.base === '?',
          isDeletedFile: f.base === 'D',
          hunks
        });
      });
    },
    async log(tabId, limit, query) {
      await delay();
      const r = repoOf(tabId);
      const q = query?.toLowerCase();
      return clone(r.commits.filter((c) => !q || c.subject.toLowerCase().includes(q)).slice(0, limit));
    },
    async createBranch(tabId, name) {
      const r = repoOf(tabId);
      await delay();
      r.branches.push(branch(name, r.branches.length + 60, false, null));
      publish(r);
    },
    async renameBranch(tabId, oldName, newName) {
      const r = repoOf(tabId);
      await delay();
      r.branches = r.branches.map((b) => (b.name === oldName ? { ...b, name: newName } : b));
      publish(r);
    },
    async checkout(tabId, reference) {
      const r = repoOf(tabId);
      await delay();
      if (!r.branches.some((b) => b.name === reference)) throw err('INVALID_REFERENCE', `No existe la referencia ${reference}.`);
      r.branches = r.branches.map((b) => ({ ...b, isHead: b.name === reference }));
      r.branch = reference;
      publish(r);
      return { done: true };
    },
    async deleteBranch(tabId, name) {
      const r = repoOf(tabId);
      await delay();
      r.branches = r.branches.filter((b) => b.name !== name);
      publish(r);
    },
    async merge() {
      await delay();
      return 'upToDate';
    },
    async continueIntegration(tabId) {
      const r = repoOf(tabId);
      await delay();
      if (r.integration === 'none') throw err('NO_INTEGRATION', 'No hay ninguna integración en curso.');
      r.integration = 'none';
      publish(r);
    },
    async abortIntegration(tabId) {
      const r = repoOf(tabId);
      await delay();
      if (r.integration === 'none') throw err('NO_INTEGRATION', 'No hay ninguna integración en curso.');
      r.integration = 'none';
      r.files = r.files.filter((f) => f.base !== 'U');
      publish(r);
      return { done: true };
    },
    async getConflicts(tabId) {
      await delay();
      return clone(conflictsOf(repoOf(tabId)));
    },
    async writeConflictResolution() {
      await delay();
      return { done: true };
    },
    async markResolved(tabId, path) {
      const r = repoOf(tabId);
      await delay();
      const f = findFile(r, path);
      f.base = 'M';
      f.staged = [smallHunk(f.path, 1)];
      publish(r);
    },
    async fetch() {
      await delay(latency * 3);
    },
    async pull() {
      await delay(latency * 3);
      return 'upToDate';
    },
    async push() {
      await delay(latency * 3);
    },

    async listen<K extends keyof ApiEvents>(event: K, handler: (payload: ApiEvents[K]) => void): Promise<Unlisten> {
      let set = listeners.get(event);
      if (!set) {
        set = new Set();
        listeners.set(event, set);
      }
      set.add(handler as (p: never) => void);
      return () => {
        set?.delete(handler as (p: never) => void);
      };
    }
  };

  return api;
}
