/**
 * Validadores en tiempo de ejecución de las formas de `types.ts`. Los usa el test
 * de contrato contra los fixtures reales generados por Rust y contra `mockApi`.
 * Son ESTRICTOS con las claves: una clave nueva o ausente en Rust se detecta.
 */

import { API_ERROR_CODES } from './types';

export type Errors = string[];
type V = (v: unknown, path: string, errs: Errors) => void;

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

export const str: V = (v, p, e) => {
  if (typeof v !== 'string') e.push(`${p}: se esperaba string, llegó ${JSON.stringify(v)}`);
};
export const num: V = (v, p, e) => {
  if (typeof v !== 'number' || !Number.isFinite(v)) e.push(`${p}: se esperaba número, llegó ${JSON.stringify(v)}`);
};
export const int: V = (v, p, e) => {
  if (!Number.isInteger(v)) e.push(`${p}: se esperaba entero, llegó ${JSON.stringify(v)}`);
};
export const bool: V = (v, p, e) => {
  if (typeof v !== 'boolean') e.push(`${p}: se esperaba booleano, llegó ${JSON.stringify(v)}`);
};
export const nullable =
  (inner: V): V =>
  (v, p, e) => {
    if (v !== null) inner(v, p, e);
  };
export const optional = nullable;
export const arr =
  (inner: V): V =>
  (v, p, e) => {
    if (!Array.isArray(v)) return void e.push(`${p}: se esperaba lista`);
    v.forEach((x, i) => inner(x, `${p}[${i}]`, e));
  };
export const oneOf =
  (...allowed: string[]): V =>
  (v, p, e) => {
    if (typeof v !== 'string' || !allowed.includes(v)) e.push(`${p}: valor ${JSON.stringify(v)} fuera de ${JSON.stringify(allowed)}`);
  };

/** Objeto con claves exactas. `opt` son claves que pueden faltar (serde `skip_serializing_if`). */
export const obj =
  (shape: Record<string, V>, opt: Record<string, V> = {}): V =>
  (v, p, e) => {
    if (!isObj(v)) return void e.push(`${p}: se esperaba objeto`);
    for (const k of Object.keys(shape)) {
      if (!(k in v)) e.push(`${p}.${k}: falta`);
      else (shape[k] as V)(v[k], `${p}.${k}`, e);
    }
    for (const k of Object.keys(opt)) if (k in v) (opt[k] as V)(v[k], `${p}.${k}`, e);
    for (const k of Object.keys(v)) if (!(k in shape) && !(k in opt)) e.push(`${p}.${k}: clave desconocida (¿cambió el modelo de Rust?)`);
  };

export const either =
  (...vs: V[]): V =>
  (v, p, e) => {
    const all: string[][] = [];
    for (const f of vs) {
      const local: Errors = [];
      f(v, p, local);
      if (local.length === 0) return;
      all.push(local);
    }
    e.push(`${p}: no encaja con ninguna variante (${all.map((l) => l[0]).join(' | ')})`);
  };

/** Enum «externally tagged» de serde con una sola clave. */
export const tagged =
  (variants: Record<string, V>): V =>
  (v, p, e) => {
    if (!isObj(v)) return void e.push(`${p}: se esperaba objeto etiquetado`);
    const keys = Object.keys(v);
    if (keys.length !== 1) return void e.push(`${p}: se esperaba una única clave, hay ${JSON.stringify(keys)}`);
    const k = keys[0] as string;
    const inner = variants[k];
    if (!inner) return void e.push(`${p}: variante desconocida ${k}`);
    inner(v[k], `${p}.${k}`, e);
  };

// ---- Formas ---------------------------------------------------------------------------

export const gitPath: V = either(str, arr(int));

export const changeStatus = oneOf('.', 'M', 'A', 'D', 'R', 'C', 'U', 'T', '?', '!', '');
export const conflictKind = oneOf('bothModified', 'bothAdded', 'bothDeleted', 'addedByUs', 'addedByThem', 'deletedByUs', 'deletedByThem', 'unknown');

export const statusEntry = obj({
  path: gitPath,
  originalPath: nullable(gitPath),
  indexStatus: changeStatus,
  workTreeStatus: changeStatus,
  isConflicted: bool,
  isUntracked: bool,
  isIgnored: bool,
  isSubmodule: bool,
  conflictKind: nullable(conflictKind)
});

export const gitStatus = obj({
  branchName: nullable(str),
  headOid: nullable(str),
  isDetached: bool,
  isUnborn: bool,
  upstream: nullable(str),
  ahead: int,
  behind: int,
  entries: arr(statusEntry)
});

export const commit = obj({
  sha: str,
  parents: arr(str),
  subject: str,
  body: str,
  authorName: str,
  authorEmail: str,
  authorDateIso: str,
  refs: arr(str)
});

export const branch = obj({ name: str, oid: str, isHead: bool, upstream: nullable(str), ahead: int, behind: int });
export const remote = obj({ name: str, fetchUrl: str, pushUrl: str });

export const diffLine = obj({
  id: str,
  kind: oneOf('context', 'addition', 'deletion', 'meta'),
  text: str,
  oldLineNumber: nullable(int),
  newLineNumber: nullable(int)
});

export const diffHunk = obj({
  id: str,
  oldStart: int,
  oldCount: int,
  newStart: int,
  newCount: int,
  heading: str,
  lines: arr(diffLine)
});

export const fileDiff = obj({
  path: gitPath,
  oldPath: nullable(gitPath),
  isBinary: bool,
  isNewFile: bool,
  isDeletedFile: bool,
  hunks: arr(diffHunk)
});

const conflictSide = obj({ oid: nullable(str), content: nullable(str), isBinary: bool });
const conflictHunk = obj({ id: int, ours: str, base: nullable(str), theirs: str });
export const conflictFile = obj({
  path: gitPath,
  kind: conflictKind,
  ours: conflictSide,
  base: conflictSide,
  theirs: conflictSide,
  workingTree: nullable(str),
  hunks: arr(conflictHunk)
});

export const integrationState = oneOf('none', 'merging', 'rebasing', 'cherryPicking');

export const snapshot = obj({
  revision: int,
  status: gitStatus,
  branches: arr(branch),
  remoteBranches: arr(branch),
  remotes: arr(remote),
  integration: integrationState,
  conflicts: arr(conflictFile)
});

export const recentRepository = obj(
  { isFavorite: bool, lastOpened: str, name: str, path: str },
  { lastBranch: nullable(str) }
);

export const tabInfo = obj({
  tabId: str,
  path: str,
  name: str,
  branch: nullable(str),
  detached: bool,
  changeCount: int,
  conflictCount: int,
  busy: nullable(str),
  missing: bool
});

export const appState = obj({
  tabs: arr(tabInfo),
  activeTabId: nullable(str),
  recents: arr(recentRepository),
  git: nullable(obj({ path: str, version: str })),
  gitProblem: nullable(str)
});

// ---- IA, clonado y credenciales (ipc.md) ---------------------------------------------
// Aún no hay fixtures de Rust para estos tipos: los validadores los cubren igualmente
// (el mock los cumple y los usará el test de contrato cuando existan los fixtures).

export const providerState = obj({
  id: str,
  displayName: str,
  availability: oneOf('available', 'missing', 'failed'),
  version: nullable(str),
  hint: nullable(str),
  enabled: bool,
  consented: bool,
  loginCommand: str
});

export const aiState = obj({
  enabled: bool,
  profile: obj({ name: str, kind: oneOf('personal', 'empresa') }),
  choice: oneOf('auto', 'claude', 'codex', 'copilot'),
  providers: arr(providerState)
});

export const contextPreview = obj({
  provider: nullable(str),
  bytes: int,
  maxBytes: int,
  files: arr(obj({ path: gitPath, bytes: int })),
  excluded: arr(obj({ path: gitPath, reason: oneOf('sensitive', 'binary', 'too-large') })),
  redactions: int,
  recentSubjects: int
});

export const aiCommitSuggestion = obj({
  subject: str,
  body: str,
  provider: str,
  commandSummary: str,
  repoRevision: int
});

export const credentialTest = obj({ ok: bool, code: nullable(str) });

export const cloneProgress = obj({ operationId: int, phase: str, percent: nullable(num) });

export const openResult = obj({ tab: tabInfo, alreadyOpen: bool });

export const mergeResult: V = either(
  oneOf('upToDate'),
  tagged({
    fastForward: obj({ new_head: str }),
    merged: obj({ new_head: str }),
    conflicts: obj({ paths: arr(gitPath) })
  })
);

export const diffScope: V = either(
  oneOf('unstaged', 'staged'),
  tagged({ commit: str, between: obj({ base: str, head: str }) })
);

export const apiError = obj(
  { code: oneOf(...API_ERROR_CODES), message: str },
  { detail: nullable(str) }
);

const lifecycle = oneOf('opening', 'active', 'closing', 'closed');
const sessionKind: V = (v, p, e) => {
  if (!isObj(v)) return void e.push(`${p}: se esperaba objeto`);
  const variants: Record<string, V> = {
    stateChanged: obj({ type: str, state: lifecycle }),
    operationStarted: obj({ type: str, operationId: int, label: str }),
    operationRetrying: obj({ type: str, operationId: int, label: str, attempt: int, delayMs: int }),
    operationFinished: obj({ type: str, operationId: int, label: str, ok: bool, error: nullable(apiError) }),
    snapshotPublished: obj({ type: str, revision: int }),
    refreshFailed: obj({ type: str, error: apiError })
  };
  const inner = variants[String(v.type)];
  if (!inner) return void e.push(`${p}.type: variante desconocida ${JSON.stringify(v.type)}`);
  inner(v, p, e);
};

export const sessionEvent = obj({ sessionId: str, revision: int, kind: sessionKind });

export function check(validator: V, value: unknown, root = '$'): Errors {
  const errs: Errors = [];
  validator(value, root, errs);
  return errs;
}

/** Detecta qué forma tiene un valor JSON a partir de sus claves (para fixtures sin tipo declarado). */
export function detect(value: unknown): { name: string; validator: V } | null {
  const probe = Array.isArray(value) ? value[0] : value;
  if (Array.isArray(value) && value.length === 0) return null;
  if (!isObj(probe)) return null;
  const has = (...ks: string[]) => ks.every((k) => k in probe);
  const list = Array.isArray(value);
  if (has('code', 'message')) return { name: list ? 'ApiError[]' : 'ApiError', validator: list ? arr(apiError) : apiError };
  if (has('sessionId', 'kind')) return { name: list ? 'SessionEvent[]' : 'SessionEvent', validator: list ? arr(sessionEvent) : sessionEvent };
  if (!list && has('revision', 'status', 'branches')) return { name: 'Snapshot', validator: snapshot };
  if (!list && has('entries', 'branchName')) return { name: 'GitStatus', validator: gitStatus };
  if (!list && has('tabs', 'recents')) return { name: 'AppState', validator: appState };
  if (has('hunks', 'isBinary')) return { name: list ? 'FileDiff[]' : 'FileDiff', validator: list ? arr(fileDiff) : fileDiff };
  if (has('sha', 'subject')) return { name: list ? 'Commit[]' : 'Commit', validator: list ? arr(commit) : commit };
  if (has('indexStatus', 'workTreeStatus')) return { name: list ? 'StatusEntry[]' : 'StatusEntry', validator: list ? arr(statusEntry) : statusEntry };
  if (has('fetchUrl', 'pushUrl')) return { name: list ? 'Remote[]' : 'Remote', validator: list ? arr(remote) : remote };
  if (has('isHead', 'oid')) return { name: list ? 'Branch[]' : 'Branch', validator: list ? arr(branch) : branch };
  if (!list && has('subject', 'body', 'provider', 'repoRevision')) return { name: 'AiCommitSuggestion', validator: aiCommitSuggestion };
  if (!list && has('enabled', 'profile', 'providers')) return { name: 'AiState', validator: aiState };
  if (!list && has('maxBytes', 'redactions')) return { name: 'ContextPreview', validator: contextPreview };
  if (!list && has('operationId', 'phase')) return { name: 'CloneProgress', validator: cloneProgress };
  if (has('lastOpened', 'isFavorite')) return { name: list ? 'RecentRepository[]' : 'RecentRepository', validator: list ? arr(recentRepository) : recentRepository };
  if (has('tabId', 'missing')) return { name: list ? 'TabInfo[]' : 'TabInfo', validator: list ? arr(tabInfo) : tabInfo };
  if (has('ours', 'theirs', 'workingTree')) return { name: list ? 'ConflictFile[]' : 'ConflictFile', validator: list ? arr(conflictFile) : conflictFile };
  if (has('oldStart', 'newStart', 'lines')) return { name: list ? 'DiffHunk[]' : 'DiffHunk', validator: list ? arr(diffHunk) : diffHunk };
  return null;
}
