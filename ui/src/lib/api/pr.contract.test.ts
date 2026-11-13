import { describe, expect, it } from 'vitest';
import { check } from './validate';
import {
  prAction,
  prDiff,
  prReviewThread,
  prSupport,
  pullRequestDetail,
  pullRequestPage
} from './validatePr';
import { arr } from './validate';
import type { PrAction, PrDiff, PrReviewThread, PrSupport, PullRequestDetail, PullRequestPage } from './prTypes';

// Fixtures REALES generados por `src-tauri` (test `pull_requests`) con la serialización de
// `ariadna-hosting`. Un cambio en los modelos de Rust rompe aquí.
const fx = import.meta.glob('./fixtures-pr/*.json', { eager: true, import: 'default' }) as Record<string, unknown>;
const get = (name: string): unknown => {
  const e = Object.entries(fx).find(([f]) => f.endsWith(`/${name}`));
  if (!e) throw new Error(`falta el fixture ${name}`);
  return e[1];
};

describe('contrato de pull requests con Rust: fixtures', () => {
  const cases: [string, Parameters<typeof check>[0]][] = [
    ['pr-detail.json', pullRequestDetail],
    ['pr-page.json', pullRequestPage],
    ['pr-support.json', prSupport],
    ['pr-support-github.json', prSupport],
    ['pr-threads.json', arr(prReviewThread)],
    ['pr-diff.json', prDiff],
    ['pr-action.json', prAction],
    ['pr-action-declined.json', prAction]
  ];
  for (const [name, validator] of cases) {
    it(`${name} encaja con prTypes.ts`, () => {
      const errs = check(validator, get(name), name);
      expect(errs, errs.join('\n')).toEqual([]);
    });
  }

  it('los fixtures tienen el tipo TypeScript esperado (compila)', () => {
    const detail = get('pr-detail.json') as PullRequestDetail;
    const page = get('pr-page.json') as PullRequestPage;
    const support = get('pr-support.json') as PrSupport;
    const threads = get('pr-threads.json') as PrReviewThread[];
    const diff = get('pr-diff.json') as PrDiff;
    const action = get('pr-action.json') as PrAction;
    expect(detail.state).toBe('active');
    expect(page.hasMore).toBe(true);
    expect(support.capabilities?.provider).toBe('azureDevOps');
    expect(threads[1]?.anchor).toBeNull();
    expect(diff.diffs[0]?.hunks[0]?.lines.length).toBe(2);
    expect(action.done).toBe(true);
  });

  it('el validador detecta claves nuevas o ausentes', () => {
    const d = structuredClone(get('pr-detail.json')) as Record<string, unknown>;
    delete d.title;
    d.inventada = 1;
    const errs = check(pullRequestDetail, d, 'pr');
    expect(errs.some((e) => e.includes('title'))).toBe(true);
    expect(errs.some((e) => e.includes('inventada'))).toBe(true);
  });

  it('ningún fixture contiene nada parecido a un token', () => {
    const text = JSON.stringify(fx);
    expect(text).not.toMatch(/ghp_|github_pat_|Authorization|Bearer |password|secret/i);
  });
});
