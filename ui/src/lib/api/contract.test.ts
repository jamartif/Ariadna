import { describe, expect, it } from 'vitest';
import { createMockApi } from './mockApi';
import { toApiError, ApiError, API_ERROR_CODES } from './types';
import { appState, check, detect, diffScope, fileDiff, mergeResult, recentRepository, snapshot, gitPath } from './validate';

// Fixtures REALES generados por `src-tauri` desde el motor de Rust (ipc.md, «Verificación del contrato»).
const fixtureModules = import.meta.glob('./fixtures/*.json', { eager: true, import: 'default' }) as Record<string, unknown>;
const fixtures = Object.entries(fixtureModules);

describe('contrato con Rust: fixtures', () => {
  if (fixtures.length === 0) {
    it.skip('no hay fixtures todavía (ui/src/lib/api/fixtures/*.json); el test se activará cuando existan', () => {});
    // eslint-disable-next-line no-console
    console.warn('[contrato] AVISO: no hay fixtures JSON de Rust en src/lib/api/fixtures/; test de contrato omitido.');
    return;
  }

  for (const [file, data] of fixtures) {
    it(`${file} encaja con types.ts`, () => {
      const found = detect(data);
      if (!found) {
        // eslint-disable-next-line no-console
        console.warn(`[contrato] AVISO: ${file} no se reconoce como ningún tipo conocido; sin validar.`);
        return;
      }
      const errs = check(found.validator, data, found.name);
      expect(errs, errs.join('\n')).toEqual([]);
    });
  }
});

describe('contrato con Rust: códigos de error', () => {
  const entry = Object.entries(fixtureModules).find(([f]) => f.endsWith('api-error-codes.json'));
  if (!entry) {
    it.skip('sin fixture api-error-codes.json', () => {});
    return;
  }
  const codes = (entry[1] as { code: string }[]).map((e) => e.code);
  it('cada código de Rust está en types.ts y viceversa', () => {
    expect([...codes].sort()).toEqual([...API_ERROR_CODES].sort());
  });
});

describe('contrato: validadores', () => {
  it('detectan claves desconocidas y ausentes', () => {
    const base = {
      path: 'a.txt',
      originalPath: null,
      indexStatus: 'M',
      workTreeStatus: '.',
      isConflicted: false,
      isUntracked: false,
      isIgnored: false,
      isSubmodule: false,
      conflictKind: null
    };
    const status = {
      branchName: 'main',
      headOid: null,
      isDetached: false,
      isUnborn: true,
      upstream: null,
      ahead: 0,
      behind: 0,
      entries: [base]
    };
    const okSnap = { revision: 1, status, branches: [], remoteBranches: [], remotes: [], integration: 'none', conflicts: [] };
    expect(check(snapshot, okSnap)).toEqual([]);
    expect(check(snapshot, { ...okSnap, extra: 1 }).join()).toMatch(/clave desconocida/);
    const { revision: _r, ...sinRevision } = okSnap;
    expect(check(snapshot, sinRevision).join()).toMatch(/revision: falta/);
    expect(check(snapshot, { ...okSnap, status: { ...status, entries: [{ ...base, indexStatus: 'Z' }] } }).join()).toMatch(/fuera de/);
  });

  it('GitPath acepta cadena y bytes', () => {
    expect(check(gitPath, 'a/b')).toEqual([]);
    expect(check(gitPath, [255, 254, 97])).toEqual([]);
    expect(check(gitPath, [1.5])).not.toEqual([]);
    expect(check(gitPath, 42)).not.toEqual([]);
  });

  it('DiffScope y MergeResult usan el etiquetado externo de serde', () => {
    expect(check(diffScope, 'staged')).toEqual([]);
    expect(check(diffScope, { commit: 'abc' })).toEqual([]);
    expect(check(diffScope, { between: { base: 'a', head: 'b' } })).toEqual([]);
    expect(check(diffScope, 'commit')).not.toEqual([]);
    expect(check(mergeResult, 'upToDate')).toEqual([]);
    expect(check(mergeResult, { fastForward: { new_head: 'abc' } })).toEqual([]);
    expect(check(mergeResult, { conflicts: { paths: ['a', [1, 2]] } })).toEqual([]);
    expect(check(mergeResult, { merged: { newHead: 'abc' } })).not.toEqual([]);
  });
});

describe('contrato: el backend simulado cumple las mismas formas', () => {
  it('app_state, snapshot, diff y recientes validan', async () => {
    const api = createMockApi({ latency: 0 });
    const state = await api.appState();
    expect(check(appState, state)).toEqual([]);
    for (const tab of state.tabs.filter((t) => !t.missing)) {
      const snap = await api.getSnapshot(tab.tabId);
      expect(check(snapshot, snap), tab.name).toEqual([]);
      const paths = snap.status.entries.filter((e) => !e.isIgnored && !e.isConflicted).slice(0, 12).map((e) => e.path);
      for (const scope of ['staged', 'unstaged'] as const) {
        const diffs = await api.diff(tab.tabId, scope, paths);
        expect(check((v, p, e) => diffs.forEach((d, i) => fileDiff(d, `${p}[${i}]`, e)), diffs)).toEqual([]);
      }
    }
    expect(check((v, p, e) => state.recents.forEach((r) => recentRepository(r, p, e)), 0)).toEqual([]);
  });
});

describe('ApiError', () => {
  it('normaliza lo que rechaza invoke', () => {
    const e = toApiError({ code: 'LOCKED', message: 'ocupado', detail: 'index.lock' });
    expect(e).toBeInstanceOf(ApiError);
    expect(e.code).toBe('LOCKED');
    expect(e.detail).toBe('index.lock');
    expect(toApiError('boom').code).toBe('UNKNOWN');
    expect(toApiError({ code: 'NOPE', message: 'x' }).code).toBe('UNKNOWN');
    expect(toApiError(new Error('x')).message).toBe('x');
  });
});
