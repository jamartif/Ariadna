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
  makeHunk,
  sha,
  smallHunk
} from './mockData';
import { demoConflictDefs, toConflictFile, toSummary, type MockConflictDef } from './mockConflicts';
import { hasMarkers } from '$lib/util/conflictMarkers';
import {
  ApiError,
  type AiAccount,
  type AiAccounts,
  type AiCommitSuggestion,
  type AiLoginInfo,
  type AiPullRequestSuggestion,
  type AiState,
  type AmendDraft,
  type ApiErrorCode,
  type ApiEvents,
  type AppSettings,
  type AppState,
  type Branch,
  type CliStatus,
  type Commit,
  type ConflictFile,
  type ContextPreview,
  type DiffHunk,
  type DiffScope,
  type FileDiff,
  type GitDetect,
  type GitPath,
  type IntegrationState,
  type MergeState,
  type ConflictResolution,
  type OpenResult,
  type RecentRepository,
  type Remote,
  type RemoteProviderId,
  type RemoteView,
  type SettingsView,
  type Snapshot,
  type StatusEntry,
  type TabInfo
} from './types';
import { createMockPullRequests } from './mockPr';
import { pathText } from '../util/gitpath';
import { matchRule, normalizePattern } from '../util/profileRules';
import { parseRemote, remoteBranchUrl, repoWebUrl } from '../util/remote';

const PROVIDER_IDS: Record<string, RemoteProviderId> = {
  azure: 'azureDevOps',
  github: 'github',
  gitlab: 'gitlab',
  bitbucket: 'bitbucket',
  other: 'other'
};
const PROVIDER_LABELS: Record<RemoteProviderId, string> = {
  azureDevOps: 'Azure DevOps',
  github: 'GitHub',
  gitlab: 'GitLab',
  bitbucket: 'Bitbucket',
  other: 'Otro'
};

function remoteView(r: Remote): RemoteView {
  const g = parseRemote(r.fetchUrl);
  const provider = PROVIDER_IDS[g?.provider ?? 'other'] ?? 'other';
  const canOpen = repoWebUrl(r.fetchUrl) !== null;
  return {
    name: r.name,
    fetchUrl: r.fetchUrl,
    pushUrl: r.pushUrl,
    provider,
    providerLabel: PROVIDER_LABELS[provider],
    host: g ? (g.provider === 'azure' ? 'dev.azure.com' : g.host) : '',
    canOpen,
    canCreatePullRequest: canOpen && provider !== 'other',
    ssh: g?.scheme === 'ssh'
  };
}

/** Misma política que `check_new_remote_url` de Rust (el mock debe fallar igual). */
function badRemoteUrl(url: string): string | null {
  const u = url.trim();
  if (u === '' || u.startsWith('-') || /[\u0000-\u001f\u007f]/.test(u)) return 'La dirección del remoto no es válida.';
  if (/^[A-Za-z0-9+.-]+::/.test(u)) return 'Ese tipo de dirección (transporte auxiliar de git) no se admite.';
  const m = /^[^:/]+:\/\/([^/?#]*)/.exec(u);
  const auth = m?.[1] ?? '';
  const at = auth.lastIndexOf('@');
  if (at >= 0 && auth.slice(0, at).includes(':')) return 'La dirección no puede llevar la contraseña o el token.';
  return null;
}
const REMOTE_NAME = /^[A-Za-z0-9_][A-Za-z0-9._/-]*$/;

export interface MockOptions {
  /** Latencia simulada por llamada en ms (por defecto 120; 0 en tests). */
  latency?: number;
  /**
   * `empty`, `nogit`, `slow`, `error` y los de IA: `ai-slow`, `ai-not-installed`,
   * `ai-not-logged`, `ai-quota`, `ai-timeout`, `ai-bad-output`, `ai-too-large`,
   * `ai-sensitive`, `ai-disabled`, `ai-consent-denied`, `ai-stale`.
   */
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
  /** Resuelto durante el merge en curso (el aborto lo deshace). */
  fromMerge?: boolean;
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
  /** Rama que se integra (merge en curso). */
  mergeSource?: string;
  /** Contenido de los archivos en conflicto (tres lados + marcadores). */
  conflictDefs?: Map<string, MockConflictDef>;
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

/**
 * Historial sintético con ramas y merges (bloques de 5 commits) para probar el grafo y la
 * virtualización. Los padres siempre vienen después del hijo (orden topológico, como `git log`).
 */
/** Hash de 40 hex único por índice (el generador de `sha()` colisiona con semillas grandes). */
function uniqueSha(i: number): string {
  return sha(i + 7).slice(0, 32) + i.toString(16).padStart(8, '0');
}

export function richHistory(count: number, baseDate = '2026-11-06T10:00:00+01:00'): Commit[] {
  const base = new Date(baseDate).getTime();
  const authors = ['Javier', 'Ana Ruiz', 'Marcos Peña', 'bot-ci'];
  const out: Commit[] = [];
  for (let i = 0; i < count; i++) {
    const k = i % 5;
    const b = i - k;
    const at = (j: number) => (j < count ? [uniqueSha(j)] : []);
    let parents: string[];
    if (k === 0) parents = [...at(b + 1), ...at(b + 3)];
    else if (k === 1) parents = at(b + 2);
    else if (k === 2) parents = at(b + 5);
    else if (k === 3) parents = at(b + 4);
    else parents = at(b + 5);
    const author = authors[i % authors.length] ?? 'Javier';
    out.push({
      sha: uniqueSha(i),
      parents,
      subject: k === 0 ? `Fusiona la rama feature/tarea-${Math.floor(i / 5)}` : `${['Corrige', 'Añade', 'Refactoriza', 'Documenta'][i % 4]} el módulo ${i % 37} (#${count - i})`,
      body: i % 11 === 0 ? 'Detalle largo del cambio.\n\nIncluye pruebas y notas de migración.' : '',
      authorName: author,
      authorEmail: `${author.toLowerCase().replace(/[^a-z]+/g, '.')}@example.com`,
      authorDateIso: new Date(base - i * 3_600_000 * 7).toISOString(),
      refs: i === 0 ? ['HEAD -> main', 'origin/main'] : i === 3 ? ['origin/feature/payments'] : i === 40 ? ['tag: v1.2.0'] : []
    });
  }
  return out;
}

/** Archivos de ejemplo de un commit simulado (determinista por hash). */
function commitFiles(c: Commit): FileDiff[] {
  const pool: [string, string][] = [
    ['src/lib/api/types.ts', 'export type GitPath'],
    ['crates/ariadna-git/src/models.rs', 'pub struct Commit'],
    ['ui/src/App.svelte', ''],
    ['docs/ipc.md', '## Comandos'],
    ['README.md', '']
  ];
  const seed = parseInt(c.sha.slice(0, 6), 16);
  const n = 1 + (seed % 3);
  return Array.from({ length: n }, (_, i) => {
    const [path, heading] = pool[(seed + i * 2) % pool.length] as [string, string];
    return {
      path,
      oldPath: null,
      isBinary: false,
      isNewFile: i === 2,
      isDeletedFile: false,
      hunks: [
        makeHunk(`${c.sha.slice(0, 7)}#${i}`, 10, 10, heading, [
          ' contexto anterior',
          `-versión antigua de la línea ${seed % 90}`,
          `+${c.subject}`,
          '+línea añadida por este commit',
          ' contexto posterior'
        ])
      ]
    };
  });
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
      conflicted('config/settings.json', 'bothAdded'),
      conflicted('assets/logo.png', 'bothModified'),
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
    mergeSource: 'origin/main',
    conflictDefs: demoConflictDefs(),
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
    commits: richHistory(1500),
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
export type MockApi = Api & {
  __repos: Map<string, MockRepo>;
  __lastPullRebase: () => boolean | null;
  /** Enlaces que habría abierto `openRemoteUrl`, en orden. */
  __openedUrls: () => string[];
  /** Solo pruebas: inicios de sesión de IA pedidos, en orden. */
  __aiLogins: () => AiLoginInfo[];
  /** Simula un elemento del menú nativo (`menu-action`). */
  __menuAction: (id: string) => void;
};

export function createMockApi(options: MockOptions = {}): MockApi {
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

  // ---- IA simulada (ipc.md «Comandos de IA») ----
  let ai: AiState = {
    enabled: scenario !== 'ai-disabled',
    profile: { name: 'Personal', kind: 'personal', isolated: false },
    choice: 'auto',
    providers: [
      { id: 'claude', displayName: 'Claude', availability: scenario === 'ai-not-installed' ? 'missing' : 'available', version: scenario === 'ai-not-installed' ? null : '2.1.319', hint: scenario === 'ai-not-logged' ? 'Sin sesión iniciada' : null, enabled: true, consented: false, loginCommand: 'claude auth login', sharesLogin: false, isolated: false, isolationVerified: true },
      { id: 'codex', displayName: 'Codex', availability: 'missing', version: null, hint: 'No se encontró el comando «codex»', enabled: true, consented: false, loginCommand: 'codex login', sharesLogin: false, isolated: false, isolationVerified: false },
      { id: 'copilot', displayName: 'Copilot', availability: 'failed', version: '1.0.4', hint: 'Sin sesión iniciada', enabled: true, consented: false, loginCommand: 'copilot login', sharesLogin: true, isolated: false, isolationVerified: false }
    ]
  };
  /** Aislamiento de sesión por perfil: Personal conserva el login normal, Empresa usa el suyo. */
  const isolationPref: Record<'personal' | 'empresa', boolean> = { personal: false, empresa: true };
  /** Inicios de sesión pedidos (solo pruebas). */
  const aiLogins: AiLoginInfo[] = [];
  const withIsolation = (st: AiState): AiState => {
    const on = isolationPref[st.profile.kind];
    return { ...st, profile: { ...st.profile, isolated: on }, providers: st.providers.map((p) => ({ ...p, isolated: on && !p.sharesLogin })) };
  };
  // ---- Ajustes simulados (ipc.md «Ajustes») ----
  let settingsData: AppSettings = {
    version: 1,
    language: 'es',
    theme: 'system',
    diffFontSize: 13,
    gitPath: null,
    pullRebase: false,
    profileRules: [
      { pattern: 'dev.azure.com', profile: 'empresa' },
      { pattern: 'github.com', profile: 'personal' }
    ]
  };
  let gitInUse: { path: string; version: string } | null = scenario === 'nogit' ? null : { path: '/usr/bin/git', version: '2.47.0' };
  const settingsWarning =
    scenario === 'settings-corrupt' ? 'settings.json no se pudo leer (expected value at line 1 column 1). Se guardó una copia como settings.json.bak y se usan los valores por defecto.' : null;
  /** Último `pull` pedido (para comprobar en tests que respeta el ajuste). */
  let lastPullRebase: boolean | null = null;
  const cliPaths: Record<string, string> = { claude: '/usr/local/bin/claude', copilot: '/usr/local/bin/copilot' };
  /** Cancelaciones pendientes por pestaña (`ai_cancel`). */
  const aiCancelled = new Map<string, () => void>();
  /** Credenciales «guardadas» por host: solo si el token era válido; el secreto NO se conserva. */
  const credentials = new Map<string, { username: string; valid: boolean }>();
  const cloneOps = new Map<number, () => void>();
  /** Enlaces que habría abierto el navegador (solo pruebas). */
  const openedUrls: string[] = [];

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
      .map((f) => {
        const def = r.conflictDefs?.get(f.path);
        if (def) return toConflictFile(f.path, def);
        return {
          path: f.path,
          kind: f.conflictKind ?? 'bothModified',
          ours: { oid: sha(1), content: 'ours\n', isBinary: false },
          base: { oid: sha(2), content: 'base\n', isBinary: false },
          theirs: { oid: sha(3), content: 'theirs\n', isBinary: false },
          workingTree: '<<<<<<< ours\nours\n=======\ntheirs\n>>>>>>> theirs\n',
          hunks: [{ id: 0, ours: 'ours\n', base: null, theirs: 'theirs\n' }]
        };
      });
  }

  function mergeStateOf(r: MockRepo): MergeState {
    const conflicts = r.files
      .filter((f) => f.base === 'U')
      .map((f) => toSummary(f.path, r.conflictDefs?.get(f.path) ?? { kind: f.conflictKind ?? 'bothModified', ours: 'ours\n', base: 'base\n', theirs: 'theirs\n', working: '<<<<<<< ours\nours\n=======\ntheirs\n>>>>>>> theirs\n' }));
    return {
      integration: r.integration,
      otherHead: r.integration === 'none' ? null : sha(77),
      sourceLabel: r.mergeSource ?? null,
      message: r.integration === 'none' ? '' : `Merge branch '${r.mergeSource ?? 'rama'}' into ${r.branch ?? 'HEAD'}\n`,
      conflicts,
      canContinue: r.integration !== 'none' && conflicts.length === 0
    };
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

  /** `ai` con la regla repo → perfil de la pestaña aplicada (como hace Rust). */
  function withRule(tabId: string | undefined): AiState {
    const out = clone(ai);
    const r = tabId ? repos.get(tabId) : undefined;
    const url = r?.remotes.find((x) => x.name === 'origin')?.fetchUrl ?? r?.remotes[0]?.fetchUrl;
    const m = url ? matchRule(settingsData.profileRules, url) : null;
    if (m) {
      out.profile = { name: m.profile === 'empresa' ? 'Empresa' : 'Personal', kind: m.profile, isolated: false, rule: m.pattern };
      out.providers = out.providers.map((p) => (p.id === 'claude' ? { ...p, enabled: m.profile !== 'empresa' } : p));
    }
    return withIsolation(out);
  }

  /** Perfil de una petición de cuenta/login: el pedido, si no el de la regla de la pestaña, si no el activo. */
  function profileOf(tabId: string | undefined, profile: 'personal' | 'empresa' | undefined): { kind: 'personal' | 'empresa'; rule?: string } {
    if (profile) return { kind: profile };
    const st = withRule(tabId);
    return { kind: st.profile.kind, rule: st.profile.rule };
  }

  function mockConfigDir(kind: 'personal' | 'empresa', provider: string): string | null {
    const p = ai.providers.find((x) => x.id === provider);
    if (!p || p.sharesLogin || !isolationPref[kind]) return null;
    return `/home/javier/.local/share/ariadna/ai-profiles/${kind}/${provider}`;
  }

  function badSetting(field: string, why: string): ApiError {
    return err('PARSE', `Ajuste no válido «${field}»: ${why}.`, field);
  }

  function settingsView(): SettingsView {
    return {
      settings: clone(settingsData),
      load: { status: settingsWarning ? 'resetCorrupt' : 'ok', warning: settingsWarning },
      about: { version: '0.1.0-alpha.1', license: 'MIT', repository: 'https://github.com/jamartif/Ariadna', issues: 'https://github.com/jamartif/Ariadna/issues' },
      git: gitInUse ? { ...gitInUse } : null,
      gitProblem: gitInUse ? null : 'No se encontró Git en el equipo. Ariadna necesita Git 2.39 o posterior.'
    };
  }

  function probeGit(path: string | null): GitDetect {
    if (scenario === 'nogit' && path === null) {
      return { ok: false, source: 'auto', path: null, version: null, problem: 'No se encontró git. Instala Git (https://git-scm.com/downloads) o indica su ruta en Ajustes.' };
    }
    if (path === null) return { ok: true, source: 'auto', path: '/usr/bin/git', version: '2.47.0', problem: null };
    if (!/^([a-z]:[\\/]|\/)/i.test(path)) {
      return { ok: false, source: 'configured', path: null, version: null, problem: 'La ruta debe ser absoluta (p. ej. C:\\Program Files\\Git\\cmd\\git.exe).' };
    }
    if (/git(\.exe)?$/i.test(path) && !/viejo|nada|missing/i.test(path)) {
      return { ok: true, source: 'configured', path, version: /git-2\.3\d/.test(path) ? '2.36.0' : '2.47.0', problem: null };
    }
    return { ok: false, source: 'configured', path: null, version: null, problem: `No hay un ejecutable de git en «${path}».` };
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

  const api: MockApi = {
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
      const warning = order.length > 20 ? `Tienes ${order.length} pestañas abiertas. Más de 20 puede ir lento: cierra las que no uses.` : null;
      return { tab: tabInfo(repo), alreadyOpen: false, warning };
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
    async cancelOperation(_tabId, operationId) {
      const stop = cloneOps.get(operationId);
      if (!stop) return false;
      stop();
      return true;
    },
    async openExternalUrl(url) {
      await delay(Math.min(latency, 40));
      let u: URL;
      try {
        u = new URL(url);
      } catch {
        throw err('POLICY', 'La dirección no es válida.');
      }
      const ok = ['learn.microsoft.com', 'dev.azure.com', 'github.com', 'docs.github.com', 'gitlab.com', 'docs.gitlab.com', 'bitbucket.org', 'support.atlassian.com'];
      if (u.protocol !== 'https:' || !(ok.includes(u.hostname) || u.hostname.endsWith('.visualstudio.com'))) {
        throw err('POLICY', 'Ariadna solo abre enlaces https de proveedores conocidos.');
      }
      if (typeof window !== 'undefined' && typeof window.open === 'function' && scenario !== 'test') {
        try {
          window.open(url, '_blank', 'noopener,noreferrer');
        } catch {
          /* jsdom: no implementado */
        }
      }
    },
    async cloneRepository(url, destination, branchArg, auth): Promise<OpenResult> {
      const operationId = ++opCounter;
      let cancelled = false;
      cloneOps.set(operationId, () => {
        cancelled = true;
      });
      const phases: [string, number | null][] = [
        ['Resolviendo el host', null],
        ['Recibiendo objetos', 12],
        ['Recibiendo objetos', 47],
        ['Recibiendo objetos', 83],
        ['Resolviendo deltas', 100]
      ];
      try {
        const host = (/^[a-z]+:\/\/(?:[^@/]*@)?([^/:]+)/i.exec(url)?.[1] ?? /@([^:]+):/.exec(url)?.[1] ?? '').toLowerCase();
        const needsAuth = /private|secreto/i.test(url) || host === 'dev.azure.com' || host.endsWith('.visualstudio.com') || host === 'ssh.dev.azure.com';
        const step = Math.max(0, Math.min(latency, 400));
        for (const [phase, percent] of phases) {
          emit('clone-progress', { operationId, phase, percent });
          await delay(step || 0);
          if (cancelled) throw err('CANCELLED', 'Clonado cancelado.');
          if (phase === 'Resolviendo el host' && needsAuth) {
            // El PAT de un solo uso se valida y se descarta: nunca se guarda en `credentials`.
            const oneShot = auth && auth.secret.length >= 8 && auth.secret !== 'invalid-token';
            const cred = credentials.get(host);
            if (!oneShot && (!cred || !cred.valid)) {
              throw err('AUTH', 'Git no pudo autenticarse en el servidor. Guarda un token de acceso personal (PAT) o inicia sesión con el Git Credential Manager.', `remote: Authentication failed for '${host}'`);
            }
          }
        }
        if (/no-existe|notfound/i.test(url)) throw err('COMMAND_FAILED', 'Git no pudo clonar el repositorio: no se encontró.', 'fatal: repository not found');
        const name = destination.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || 'repo';
        const existing = [...repos.values()].find((r) => r.path === destination);
        let repo = existing;
        let alreadyOpen = true;
        if (!repo) {
          alreadyOpen = false;
          repo = buildSmall(destination, newCounter++);
          repo.name = name;
          repo.branch = branchArg && branchArg.trim() !== '' ? branchArg.trim() : 'main';
          repo.files = [];
          repo.branches = [branch(repo.branch, 40 + newCounter, true, null)];
          repo.remotes = [{ name: 'origin', fetchUrl: url, pushUrl: url }];
          repos.set(repo.tabId, repo);
          order.push(repo.tabId);
        }
        activeTabId = repo.tabId;
        touchRecent(repo);
        emit('tabs-changed', appState());
        return { tab: tabInfo(repo), alreadyOpen };
      } finally {
        cloneOps.delete(operationId);
      }
    },
    async storeCredential(url, username, secret) {
      await delay(Math.min(latency, 80));
      if (username.trim() === '' || secret === '') throw err('PARSE', 'Falta el usuario o el token.');
      const host = (/^[a-z]+:\/\/(?:[^@/]*@)?([^/:]+)/i.exec(url)?.[1] ?? '').toLowerCase();
      // El secreto no se conserva: solo se recuerda si «era válido» para simular el servidor.
      credentials.set(host, { username, valid: secret.length >= 8 && secret !== 'invalid-token' });
    },
    async testCredential(url) {
      await delay(Math.min(latency * 4, 700));
      const host = (/^[a-z]+:\/\/(?:[^@/]*@)?([^/:]+)/i.exec(url)?.[1] ?? '').toLowerCase();
      const c = credentials.get(host);
      if (c?.valid) return { ok: true, code: null };
      const needs = /private|secreto/i.test(url) || host === 'dev.azure.com' || host.endsWith('.visualstudio.com');
      return needs || c ? { ok: false, code: 'AUTH' } : { ok: true, code: null };
    },
    async forgetCredential(url, username) {
      await delay(Math.min(latency, 60));
      const host = (/^[a-z]+:\/\/(?:[^@/]*@)?([^/:]+)/i.exec(url)?.[1] ?? '').toLowerCase();
      if (credentials.get(host)?.username === username) credentials.delete(host);
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
      if (typeof scope !== 'string') {
        if ('commit' in scope) {
          const c = r.commits.find((x) => x.sha === scope.commit || x.sha.startsWith(scope.commit));
          return c ? clone(commitFiles(c)) : [];
        }
        return [];
      }
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
    async logPage(tabId, skip, limit, query) {
      await delay();
      const r = repoOf(tabId);
      const q = query?.trim().toLowerCase();
      const list = r.commits.filter(
        (c) => !q || c.subject.toLowerCase().includes(q) || c.authorName.toLowerCase().includes(q) || c.sha.startsWith(q)
      );
      return clone(list.slice(skip, skip + Math.min(limit, 500)));
    },
    async createBranch(tabId, name) {
      const r = repoOf(tabId);
      await delay();
      if (r.branches.some((b) => b.name === name)) throw err('COMMAND_FAILED', `Ya existe una rama llamada ${name}.`, `fatal: a branch named '${name}' already exists`);
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
      if (r.branches.find((b) => b.name === name)?.isHead) {
        throw err('COMMAND_FAILED', 'No se puede borrar la rama actual.', `error: Cannot delete branch '${name}' checked out`);
      }
      if (!confirmFn(`¿Borrar la rama «${name}» de ${r.name}?`)) return { done: false };
      r.branches = r.branches.filter((b) => b.name !== name);
      publish(r);
      return { done: true };
    },
    async merge() {
      await delay();
      return 'upToDate';
    },
    async mergeClean(tabId, branchName) {
      const r = repoOf(tabId);
      await delay();
      if (!r.branches.some((b) => b.name === branchName) && !r.remoteBranches.some((b) => b.name === branchName)) {
        throw err('INVALID_REFERENCE', `No existe la referencia ${branchName}.`);
      }
      if (/conflict|choca/i.test(branchName)) {
        throw err('MERGE_CONFLICTS', `Hay conflictos al fusionar «${branchName}» (2 archivo(s)). La fusión se ha cancelado y no se ha cambiado nada. Usa «merge_start» para dejarla en curso y resolver los conflictos.`, 'src/lib/api/types.ts\ndocs/ipc.md');
      }
      const target = r.branches.find((b) => b.name === branchName);
      if (target && r.branches.find((b) => b.isHead)?.oid === target.oid) return 'upToDate';
      const newHead = sha(r.commits.length + 90 + r.revision);
      const c: Commit = {
        sha: newHead,
        parents: [r.commits[0]?.sha ?? ''].filter(Boolean),
        subject: `Fusiona la rama '${branchName}' en ${r.branch ?? 'HEAD'}`,
        body: '',
        authorName: 'Javier',
        authorEmail: 'javier@example.com',
        authorDateIso: now().toISOString(),
        refs: [`HEAD -> ${r.branch ?? 'main'}`]
      };
      r.commits = [c, ...r.commits.map((x) => ({ ...x, refs: x.refs.filter((f) => !f.startsWith('HEAD')) }))];
      publish(r);
      return { merged: { new_head: newHead } };
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
      r.files = r.files.filter((f) => f.base !== 'U' && !f.fromMerge);
      r.conflictDefs = undefined;
      publish(r);
      return { done: true };
    },
    async mergeStart(tabId, branchName, fastForward) {
      const r = repoOf(tabId);
      if (r.integration !== 'none') throw err('COMMAND_FAILED', 'Ya hay una integración en curso.', 'fatal: You have not concluded your merge (MERGE_HEAD exists).');
      if (!/conflict|choca/i.test(branchName)) {
        return { result: await api.mergeClean(tabId, branchName, fastForward), state: null };
      }
      await delay();
      if (!r.branches.some((b) => b.name === branchName) && !r.remoteBranches.some((b) => b.name === branchName)) {
        throw err('INVALID_REFERENCE', `No existe la referencia ${branchName}.`);
      }
      const defs = demoConflictDefs();
      r.integration = 'merging';
      r.mergeSource = branchName;
      r.conflictDefs = defs;
      for (const [path, d] of defs) {
        r.files = r.files.filter((f) => f.path !== path);
        r.files.unshift({ path, base: 'U', staged: [], unstaged: [], conflictKind: d.kind });
      }
      publish(r);
      return { result: { conflicts: { paths: [...defs.keys()] } }, state: mergeStateOf(r) };
    },
    async mergeState(tabId) {
      await delay(Math.min(latency, 60));
      return clone(mergeStateOf(repoOf(tabId)));
    },
    async resolveConflict(tabId, path, resolution: ConflictResolution) {
      const r = repoOf(tabId);
      await delay();
      const f = r.files.find((x) => x.path === pathText(path) && x.base === 'U');
      if (!f) throw err('CONFLICT_NOT_FOUND', 'Ese archivo ya no está en conflicto.', pathText(path));
      const def = r.conflictDefs?.get(f.path);
      const binary = def?.binary === true;
      let deleting = false;
      if ('side' in resolution) {
        const content = resolution.side === 'ours' ? def?.ours : resolution.side === 'theirs' ? def?.theirs : def?.base;
        if (content === null || content === undefined) {
          if (resolution.side === 'base') throw err('CONFLICT_NOT_FOUND', 'Este conflicto no tiene versión base.', f.path);
          deleting = true;
        }
      } else if (binary) {
        throw err('CONFLICT_NOT_TEXT', 'Este archivo es binario o no es texto UTF-8: solo se puede elegir una versión entera.', f.path);
      } else if ('manual' in resolution) {
        if (hasMarkers(new TextDecoder().decode(new Uint8Array(resolution.manual)))) {
          throw err('CONFLICT_MARKERS_REMAIN', 'Quedan marcadores de conflicto (<<<<<<<, =======, >>>>>>>) sin resolver en el resultado.');
        }
      }
      if (!confirmFn(`¿${deleting ? 'Borrar' : 'Sobrescribir'} ${f.path} con la resolución elegida y marcarlo como resuelto?`)) return { done: false };
      if (deleting) {
        r.files = r.files.filter((x) => x !== f);
      } else {
        f.base = 'M';
        f.conflictKind = undefined;
        f.fromMerge = true;
        f.staged = [smallHunk(f.path, 1)];
      }
      r.conflictDefs?.delete(f.path);
      publish(r);
      return { done: true };
    },
    async mergeContinue(tabId, message) {
      const r = repoOf(tabId);
      await delay();
      if (r.integration === 'none') throw err('NO_INTEGRATION', 'No hay ninguna integración en curso.');
      const pending = r.files.filter((f) => f.base === 'U');
      if (pending.length > 0) {
        throw err('CONFLICTS_PENDING', `Quedan ${pending.length} archivo(s) en conflicto sin resolver.`, pending.map((f) => f.path).join('\n'));
      }
      const subject = (message ?? '').trim() === '' ? `Merge branch '${r.mergeSource ?? 'rama'}' into ${r.branch ?? 'HEAD'}` : (message ?? '').trim().split('\n')[0] ?? '';
      const newHead = sha(r.commits.length + 120 + r.revision);
      r.commits = [
        {
          sha: newHead,
          parents: [r.commits[0]?.sha ?? ''].filter(Boolean),
          subject,
          body: '',
          authorName: 'Javier',
          authorEmail: 'javier@example.com',
          authorDateIso: now().toISOString(),
          refs: [`HEAD -> ${r.branch ?? 'main'}`]
        },
        ...r.commits.map((x) => ({ ...x, refs: x.refs.filter((ref) => !ref.startsWith('HEAD')) }))
      ];
      for (const f of r.files.filter((x) => x.fromMerge)) {
        r.files = r.files.filter((x) => x !== f);
      }
      r.integration = 'none';
      r.conflictDefs = undefined;
      publish(r);
      return { done: true, sha: newHead, integration: 'none' };
    },
    async mergeAbort(tabId) {
      const r = repoOf(tabId);
      await delay();
      if (r.integration !== 'merging') throw err('NO_INTEGRATION', 'No hay ningún merge en curso.');
      if (!confirmFn('¿Abortar el merge? Se perderá lo resuelto hasta ahora.')) return { done: false };
      return api.abortIntegration(tabId);
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
    async pull(_tabId, _remote, _refspec, rebase) {
      await delay(latency * 3);
      lastPullRebase = rebase ?? settingsData.pullRebase;
      return 'upToDate';
    },
    async push(tabId, remote, refspec, setUpstream) {
      const r = repoOf(tabId);
      await delay(latency * 3);
      // Publicar de verdad en el simulado: aparece la rama remota y la local la sigue (la UI de PR lo necesita).
      const local = r.branches.find((x) => x.name === (refspec ?? r.branch));
      if (!local) return;
      const remoteName = `${remote}/${local.name}`;
      if (!r.remoteBranches.some((x) => x.name === remoteName)) {
        r.remoteBranches = [...r.remoteBranches, branch(remoteName, r.remoteBranches.length + 80, false, null)];
      }
      r.branches = r.branches.map((x) => (x === local ? { ...x, ahead: 0, upstream: setUpstream || !x.upstream ? remoteName : x.upstream } : x));
      publish(r);
    },

    async listRemotes(tabId) {
      await delay(Math.min(latency, 40));
      return repoOf(tabId).remotes.map(remoteView);
    },
    async addRemote(tabId, name, url) {
      const r = repoOf(tabId);
      await delay();
      const n = name.trim();
      const bad = badRemoteUrl(url) ?? (REMOTE_NAME.test(n) && !n.includes('..') ? null : 'El nombre del remoto no es válido.');
      if (bad) throw err('INVALID_REFERENCE', bad);
      if (!confirmFn(`¿Añadir el remoto «${n}» (${url.trim()}) a ${r.name}?`)) return { done: false };
      if (r.remotes.some((x) => x.name === n)) {
        throw err('COMMAND_FAILED', `Ya existe un remoto llamado «${n}».`, `error: remote ${n} already exists.`);
      }
      r.remotes = [...r.remotes, { name: n, fetchUrl: url.trim(), pushUrl: url.trim() }];
      publish(r);
      return { done: true };
    },
    async renameRemote(tabId, oldName, newName) {
      const r = repoOf(tabId);
      await delay();
      const n = newName.trim();
      if (!r.remotes.some((x) => x.name === oldName)) throw err('INVALID_REFERENCE', 'Ese remoto ya no existe.');
      if (!REMOTE_NAME.test(n) || n.includes('..')) throw err('INVALID_REFERENCE', 'El nombre nuevo del remoto no es válido.');
      if (!confirmFn(`¿Renombrar el remoto «${oldName}» a «${n}» en ${r.name}?`)) return { done: false };
      if (r.remotes.some((x) => x.name === n)) {
        throw err('COMMAND_FAILED', `Ya existe un remoto llamado «${n}».`, `error: remote ${n} already exists.`);
      }
      const from = `${oldName}/`;
      r.remotes = r.remotes.map((x) => (x.name === oldName ? { ...x, name: n } : x));
      r.remoteBranches = r.remoteBranches.map((b) => (b.name.startsWith(from) ? { ...b, name: `${n}/${b.name.slice(from.length)}` } : b));
      r.branches = r.branches.map((b) => (b.upstream?.startsWith(from) ? { ...b, upstream: `${n}/${b.upstream.slice(from.length)}` } : b));
      publish(r);
      return { done: true };
    },
    async removeRemote(tabId, name) {
      const r = repoOf(tabId);
      await delay();
      if (!r.remotes.some((x) => x.name === name)) throw err('INVALID_REFERENCE', 'Ese remoto ya no existe.');
      if (!confirmFn(`¿Quitar el remoto «${name}» de ${r.name}? Se borran sus ramas de seguimiento locales.`)) return { done: false };
      const from = `${name}/`;
      r.remotes = r.remotes.filter((x) => x.name !== name);
      r.remoteBranches = r.remoteBranches.filter((b) => !b.name.startsWith(from));
      r.branches = r.branches.map((b) => (b.upstream?.startsWith(from) ? { ...b, upstream: null, ahead: 0, behind: 0 } : b));
      publish(r);
      return { done: true };
    },
    async openRemoteUrl(tabId, remote, kind, branch) {
      const r = repoOf(tabId);
      await delay(Math.min(latency, 40));
      const rem = r.remotes.find((x) => x.name === remote);
      if (!rem) throw err('INVALID_REFERENCE', 'Ese remoto ya no existe.');
      const b = branch?.trim() ?? '';
      if (kind !== 'repo' && (b === '' || b.startsWith('-') || b.includes('..') || /\s/.test(b))) {
        throw err('INVALID_REFERENCE', 'Falta una rama válida para ese enlace.');
      }
      const url = kind === 'repo' ? repoWebUrl(rem.fetchUrl) : remoteBranchUrl(rem.fetchUrl, kind, b);
      if (!url) throw err('POLICY', `No se conoce la página web de «${remote}»: no se puede abrir ese enlace.`);
      openedUrls.push(url);
    },

    async aiState(tabId) {
      await delay(Math.min(latency, 60));
      return withRule(tabId);
    },
    async aiUpdateSettings(patch) {
      await delay(Math.min(latency, 60));
      if (patch.enabled !== undefined) ai.enabled = patch.enabled;
      if (patch.choice !== undefined) ai.choice = patch.choice as AiState['choice'];
      if (patch.profile !== undefined) {
        ai.profile = { name: patch.profile === 'empresa' ? 'Empresa' : 'Personal', kind: patch.profile, isolated: isolationPref[patch.profile] };
        // Perfil Empresa: Claude desactivado por defecto (spec §5.1.1).
        ai.providers = ai.providers.map((p) => (p.id === 'claude' ? { ...p, enabled: patch.profile !== 'empresa' } : p));
      }
      if (patch.providerEnabled) {
        const { id, enabled } = patch.providerEnabled;
        ai.providers = ai.providers.map((p) => (p.id === id ? { ...p, enabled } : p));
      }
      if (patch.isolated !== undefined) isolationPref[ai.profile.kind] = patch.isolated;
      return withRule(patch.tabId);
    },
    // Ajustes
    async getSettings() {
      await delay(Math.min(latency, 60));
      return settingsView();
    },
    async saveSettings(next) {
      await delay(Math.min(latency, 80));
      if (next.language !== 'es') throw badSetting('language', 'por ahora solo existe el español (es)');
      if (!Number.isInteger(next.diffFontSize) || next.diffFontSize < 10 || next.diffFontSize > 24) throw badSetting('diffFontSize', 'debe estar entre 10 y 24');
      if (!['system', 'light', 'dark'].includes(next.theme)) throw badSetting('theme', 'valor desconocido');
      const path = next.gitPath?.trim() || null;
      const rules = [];
      const seen = new Set<string>();
      for (const r of next.profileRules) {
        const n = normalizePattern(r.pattern);
        if (!n.ok) throw badSetting('profileRules', 'patrón no válido');
        if (seen.has(n.pattern)) throw badSetting('profileRules', 'hay dos reglas con el mismo patrón');
        seen.add(n.pattern);
        rules.push({ pattern: n.pattern, profile: r.profile });
      }
      if (path !== settingsData.gitPath) {
        const probe = probeGit(path);
        if (path !== null && !probe.ok) throw err('GIT_NOT_FOUND', `La ruta de git no es válida. ${probe.problem ?? ''}`.trim(), probe.problem ?? undefined);
        gitInUse = probe.ok && probe.path && probe.version ? { path: probe.path, version: probe.version } : null;
        emit('tabs-changed', appState());
      }
      settingsData = { ...next, version: 1, gitPath: path, profileRules: rules };
      return settingsView();
    },
    async detectGit(path) {
      await delay(Math.min(latency * 2, 300));
      return probeGit(path?.trim() ? path.trim() : null);
    },
    async gitIdentity() {
      await delay(Math.min(latency, 60));
      return scenario === 'no-identity' ? { name: null, email: null } : { name: 'Javier Martínez', email: 'javier@example.com' };
    },
    async getCliStatus(): Promise<CliStatus[]> {
      await delay(Math.min(latency * 2, 300));
      return ai.providers.map((p) => ({
        id: p.id,
        displayName: p.displayName,
        installed: p.availability === 'available',
        availability: p.availability,
        version: p.version,
        path: p.availability === 'available' ? (cliPaths[p.id] ?? `/usr/local/bin/${p.id}`) : null,
        hint: p.hint,
        loginCommand: p.loginCommand
      }));
    },
    /** Solo pruebas: lo último que recibió `pull`. */
    __lastPullRebase: () => lastPullRebase,
    /** Solo pruebas: enlaces abiertos con `openRemoteUrl`, en orden. */
    __openedUrls: () => [...openedUrls],
    /** Solo pruebas: simula el menú nativo pulsando un elemento. */
    __menuAction: (id: string) => emit('menu-action', { id }),

    async aiPreviewContext(tabId): Promise<ContextPreview> {
      await delay(Math.min(latency, 80));
      const r = repoOf(tabId);
      const staged = r.files.filter(hasStaged);
      const sizeOf = (f: MockFile) => f.staged.reduce((n, h) => n + h.lines.reduce((m, l) => m + l.text.length + 1, 0), 0) + 180;
      const sensitive = staged.filter((f) => /(^|\/)\.env|\.pem$|\.key$|id_rsa/.test(f.path));
      const binary = staged.filter((f) => f.binary);
      const big = staged.filter((f) => !sensitive.includes(f) && !binary.includes(f) && sizeOf(f) > 90_000);
      const sent = staged.filter((f) => !sensitive.includes(f) && !binary.includes(f) && !big.includes(f));
      const chosen = ai.choice === 'auto' ? ai.providers.find((p) => p.enabled && p.availability === 'available') : ai.providers.find((p) => p.id === ai.choice);
      return {
        provider: chosen?.id ?? null,
        bytes: sent.reduce((n, f) => n + sizeOf(f), 0) + (scenario === 'ai-too-large' ? 120_000 : 0),
        maxBytes: 100_000,
        files: sent.map((f) => ({ path: f.path, bytes: sizeOf(f) })),
        excluded: [
          ...sensitive.map((f) => ({ path: f.path, reason: 'sensitive' as const })),
          ...binary.map((f) => ({ path: f.path, reason: 'binary' as const })),
          ...big.map((f) => ({ path: f.path, reason: 'too-large' as const }))
        ],
        redactions: sent.length > 0 ? 2 : 0,
        recentSubjects: Math.min(20, r.commits.length)
      };
    },
    async aiGenerateCommitMessage(tabId, options): Promise<AiCommitSuggestion> {
      const r = repoOf(tabId);
      if (!ai.enabled) throw err('AI_DISABLED', 'La IA está desactivada en los ajustes de Ariadna.');
      const staged = r.files.filter(hasStaged);
      if (staged.length === 0) throw err('AI_NO_STAGED_CHANGES', 'No hay nada preparado: prepara cambios para que la IA pueda proponer un mensaje.');
      if (scenario === 'ai-sensitive') throw err('AI_SENSITIVE_FILE', 'Todo lo preparado son archivos sensibles (.env, claves) y no se envían a la IA.');
      const wanted = ai.choice === 'auto' ? (ai.providers.find((p) => p.enabled && p.availability !== 'missing') ?? ai.providers[0]) : ai.providers.find((p) => p.id === ai.choice);
      const provider = wanted as AiState['providers'][number];
      if (provider.availability === 'missing') {
        throw err('AI_NOT_INSTALLED', `No se encontró el comando «${provider.id}» en el equipo.`, provider.loginCommand);
      }
      if (provider.availability === 'failed' || scenario === 'ai-not-logged') {
        throw err('AI_NOT_LOGGED_IN', `${provider.displayName} no tiene una sesión iniciada.`, provider.loginCommand);
      }
      // Primer uso por perfil y proveedor: diálogo NATIVO de Rust (aquí, el `confirm` simulado).
      if (!provider.consented) {
        const okConsent = scenario === 'ai-consent-denied' ? false : confirmFn(`¿Permites que Ariadna envíe a ${provider.displayName} el diff preparado de ${r.name}? Se excluyen archivos sensibles y se redactan secretos.`);
        if (!okConsent) throw err('AI_CONSENT_REQUIRED', 'Sin tu permiso Ariadna no envía nada a la IA.');
        ai.providers = ai.providers.map((p) => (p.id === provider.id ? { ...p, consented: true } : p));
      }
      const op = ++opCounter;
      emit('session-event', { sessionId: r.tabId, revision: r.revision, kind: { type: 'operationStarted', operationId: op, label: `Generando mensaje con ${provider.displayName}` } });
      const think = scenario === 'ai-slow' ? 9000 : Math.max(latency * 8, latency === 0 ? 0 : 900);
      try {
        const revisionAtStart = r.revision;
        await new Promise<void>((resolve, reject) => {
          const timer = setTimeout(resolve, think);
          aiCancelled.set(tabId, () => {
            clearTimeout(timer);
            reject(err('CANCELLED', 'Generación cancelada.'));
          });
        });
        if (scenario === 'ai-quota') throw err('AI_QUOTA', `${provider.displayName} ha alcanzado su límite de uso. Vuelve a intentarlo más tarde o elige otro proveedor.`);
        if (scenario === 'ai-timeout') throw err('AI_TIMEOUT', `${provider.displayName} no respondió en 120 s.`);
        if (scenario === 'ai-bad-output') throw err('AI_BAD_OUTPUT', 'El proveedor devolvió un formato que Ariadna no reconoce.');
        if (scenario === 'ai-too-large') throw err('AI_CONTEXT_TOO_LARGE', 'Lo preparado supera el tope de 100 000 bytes que se envían a la IA.');
        if (scenario === 'ai-stale') publish(r); // el repo cambia mientras la IA piensa
        const names = staged.map((f) => f.path.split('/').pop() as string);
        const en = options.language === 'en';
        const conv = options.style === 'conventional';
        const hasTypes = staged.some((f) => f.path.endsWith('types.ts'));
        let subject = hasTypes
          ? en ? 'Add AI and clone types to the IPC contract' : 'Añade los tipos de IA y clonado al contrato IPC'
          : en ? `Update ${names.slice(0, 2).join(' and ')}` : `Actualiza ${names.slice(0, 2).join(' y ')}`;
        if (conv) subject = `${hasTypes ? 'feat(ui)' : 'chore'}: ${subject.charAt(0).toLowerCase()}${subject.slice(1)}`;
        const body = staged
          .slice(0, 4)
          .map((f) => `- ${f.path.split('/').pop()}: ${en ? (f.base === 'A' ? 'new file' : f.base === 'R' ? 'renamed' : 'updated') : f.base === 'A' ? 'archivo nuevo' : f.base === 'R' ? 'renombrado' : 'actualizado'}`)
          .join('\n');
        return {
          subject,
          body: (en ? 'Keeps the interface in sync with the backend contract.\n\n' : 'Mantiene la interfaz alineada con el contrato del backend.\n\n') + body,
          provider: provider.id,
          commandSummary: `${provider.id} -p --output-format json (stdin)`,
          repoRevision: revisionAtStart
        };
      } finally {
        aiCancelled.delete(tabId);
        emit('session-event', { sessionId: r.tabId, revision: r.revision, kind: { type: 'operationFinished', operationId: op, label: 'ai.generate', ok: true, error: null } });
      }
    },
    async aiCancel(tabId) {
      const stop = aiCancelled.get(tabId);
      if (!stop) return false;
      stop();
      return true;
    },
    async aiAccounts(request): Promise<AiAccounts> {
      await delay(Math.min(latency, 80));
      const { kind, rule } = profileOf(request?.tabId, request?.profile);
      const empresa = kind === 'empresa';
      const claude = ai.providers.find((p) => p.id === 'claude');
      const unavailable = (provider: string, reason: string): AiAccount => ({ provider, state: 'unavailable', email: null, organization: null, plan: null, authMethod: null, configDir: null, reason });
      const accounts: AiAccount[] = [
        claude?.availability === 'missing'
          ? unavailable('claude', 'No se encontró el comando «claude» en el equipo.')
          : scenario === 'ai-not-logged'
            ? { provider: 'claude', state: 'loggedOut', email: null, organization: null, plan: null, authMethod: null, configDir: mockConfigDir(kind, 'claude') ?? '/home/javier/.claude', reason: null }
            : {
                provider: 'claude',
                state: 'loggedIn',
                email: empresa ? 'javier@miempresa.com' : 'javier@example.com',
                organization: empresa ? 'Mi Empresa SL' : null,
                plan: empresa ? 'team' : 'pro',
                authMethod: 'claude.ai',
                configDir: mockConfigDir(kind, 'claude') ?? '/home/javier/.claude',
                reason: null
              },
        unavailable('codex', 'El CLI no expone la cuenta activa de forma verificada.'),
        unavailable('copilot', 'El CLI no expone la cuenta activa de forma verificada.')
      ];
      return { profile: { name: empresa ? 'Empresa' : 'Personal', kind, isolated: isolationPref[kind], ...(rule ? { rule } : {}) }, accounts };
    },
    async aiLogin(request): Promise<AiLoginInfo> {
      await delay(Math.min(latency, 80));
      const { kind } = profileOf(request.tabId, request.profile);
      const p = ai.providers.find((x) => x.id === request.provider);
      if (!p) throw err('PARSE', 'Valor no válido para «id» en los ajustes de IA.', request.provider);
      const claudeOff = kind === 'empresa' && p.id === 'claude' && !ai.providers.find((x) => x.id === 'claude')?.enabled;
      if (!p.enabled || claudeOff) throw err('AI_DISABLED', `${p.displayName} está desactivado en este perfil.`);
      if (p.availability === 'missing') throw err('AI_NOT_INSTALLED', `No se encontró el comando «${p.id}» en el equipo.`, p.loginCommand);
      const dir = mockConfigDir(kind, p.id);
      const varName = p.id === 'claude' ? 'CLAUDE_CONFIG_DIR' : p.id === 'codex' ? 'CODEX_HOME' : null;
      const notes: string[] = [];
      if (p.sharesLogin) notes.push(`${p.displayName} no permite aislar la sesión por perfil: Personal y Empresa comparten el mismo login.`);
      else if (!isolationPref[kind]) notes.push(`El perfil ${kind === 'empresa' ? 'Empresa' : 'Personal'} usa la sesión normal de ${p.displayName} (aislamiento desactivado).`);
      else if (!p.isolationVerified) notes.push(`El aislamiento de ${p.displayName} sigue la documentación del CLI y no se ha verificado.`);
      const launched = !!request.launch && scenario !== 'ai-no-terminal';
      if (request.launch && !launched) notes.push('No se pudo abrir la terminal: abrir una terminal no está soportado en este sistema.');
      const info: AiLoginInfo = {
        provider: p.id,
        profile: kind,
        command: `${dir && varName ? `${varName}='${dir}' ` : ''}${p.loginCommand}`,
        shell: 'posix',
        configDir: dir,
        isolated: !!dir,
        sharesLogin: p.sharesLogin,
        launched,
        verified: p.id === 'claude',
        note: notes.length ? notes.join(' ') : null
      };
      aiLogins.push(info);
      return info;
    },
    async aiGeneratePullRequest(tabId, options): Promise<AiPullRequestSuggestion> {
      const r = repoOf(tabId);
      if (!ai.enabled) throw err('AI_DISABLED', 'La IA está desactivada en los ajustes de Ariadna.');
      const wanted = ai.choice === 'auto' ? (ai.providers.find((p) => p.enabled && p.availability !== 'missing') ?? ai.providers[0]) : ai.providers.find((p) => p.id === ai.choice);
      const provider = wanted as AiState['providers'][number];
      if (provider.availability === 'missing') throw err('AI_NOT_INSTALLED', `No se encontró el comando «${provider.id}» en el equipo.`, provider.loginCommand);
      const base = options?.base?.trim() || 'main';
      if (base === 'no-existe') throw err('AI_BASE_NOT_FOUND', 'No encuentro la rama base para comparar.', base);
      if (r.branch === base || r.commits.length === 0) throw err('AI_NO_BRANCH_CHANGES', `La rama ${r.branch ?? 'HEAD'} no tiene cambios respecto a ${base}.`);
      if (!provider.consented) {
        const ok = scenario === 'ai-consent-denied' ? false : confirmFn(`¿Permites que Ariadna envíe a ${provider.displayName} los commits y el diff de la rama de ${r.name}?`);
        if (!ok) throw err('AI_CONSENT_REQUIRED', 'Sin tu permiso Ariadna no envía nada a la IA.');
        ai.providers = ai.providers.map((p) => (p.id === provider.id ? { ...p, consented: true } : p));
      }
      await delay(Math.max(latency * 4, latency === 0 ? 0 : 400));
      const commits = r.commits.slice(0, 3);
      return {
        title: `Mejora ${r.name}: ${commits[0]?.subject ?? 'cambios de la rama'}`.slice(0, 72),
        body: `## Resumen\n\nReúne los cambios de la rama ${r.branch ?? 'HEAD'} frente a ${base}.\n\n${commits.map((c) => `- ${c.subject}`).join('\n')}`,
        provider: provider.id,
        commandSummary: `${provider.id} -p --output-format json (stdin)`,
        repoRevision: r.revision,
        base,
        head: r.branch ?? 'HEAD',
        commitCount: r.commits.length
      };
    },
    /** Solo pruebas: inicios de sesión pedidos, en orden. */
    __aiLogins: () => [...aiLogins],

    // Pull requests (M8): datos de ejemplo en memoria, mismos comandos y errores que Rust.
    ...createMockPullRequests({
      scenario,
      remotesOf: (tabId) => repoOf(tabId).remotes.map((r) => ({ name: r.name, fetchUrl: r.fetchUrl })),
      delay: () => delay(Math.min(latency, 60)),
      confirm: (m) => confirmFn(m),
      now
    }),

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
