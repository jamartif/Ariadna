import { describe, expect, it } from 'vitest';
import type { PullRequestSummary } from '$lib/api/prTypes';
import type { Branch, Snapshot } from '$lib/api/types';
import { ApiError } from '$lib/api/types';
import { baseOptions, defaultBase, errorKind, initials, matchesQuery, parseIds, parseList, publication, retrySeconds } from './pr';

const br = (name: string, isHead = false, upstream: string | null = null, ahead = 0): Branch =>
  ({ name, isHead, upstream, ahead, behind: 0, oid: 'a'.repeat(40), subject: '' }) as unknown as Branch;

const snap = (branches: Branch[], remotes: string[]): Snapshot => ({ branches, remoteBranches: remotes.map((n) => br(n)) }) as unknown as Snapshot;

describe('util/pr', () => {
  it('iniciales de un nombre o un correo', () => {
    expect(initials('Ana')).toBe('A');
    expect(initials('Ana López')).toBe('AL');
    expect(initials('yo@ejemplo.test')).toBe('YT');
    expect(initials('')).toBe('?');
  });

  it('matchesQuery busca en título, número, autor, ramas y revisores', () => {
    const pr = {
      id: 42,
      title: 'Añade validación',
      author: { id: '1', displayName: 'Ana', login: 'ana' },
      sourceBranch: 'feature/x',
      targetBranch: 'main',
      reviewers: [{ user: { id: '2', displayName: 'Beto', login: 'beto' }, vote: 'noVote', required: false }]
    } as unknown as PullRequestSummary;
    for (const q of ['', '42', '#42', 'validación', 'ANA', 'feature', 'main', 'beto']) expect(matchesQuery(pr, q), q).toBe(true);
    expect(matchesQuery(pr, 'zzz')).toBe(false);
  });

  it('parseList y parseIds', () => {
    expect(parseList('ana, beto  ana;cris\n')).toEqual(['ana', 'beto', 'cris']);
    expect(parseIds('#12, 7 12 x9')).toEqual({ ids: [12, 7], invalid: ['x9'] });
    expect(parseIds('')).toEqual({ ids: [], invalid: [] });
  });

  it('rama base por defecto y opciones', () => {
    const s = snap([br('feature/a', true), br('main')], ['origin/main', 'origin/feature/a', 'origin/HEAD']);
    expect(defaultBase(s, 'origin', 'feature/a')).toBe('main');
    expect(baseOptions(s, 'origin', 'feature/a')).toEqual(['main']);
    expect(defaultBase(snap([], ['origin/develop']), 'origin', 'x')).toBe('develop');
    expect(defaultBase(null, null, 'main')).toBe('master');
    expect(defaultBase(null, null, 'x')).toBe('main');
  });

  it('publicación de la rama actual', () => {
    expect(publication(null, 'origin')).toBe('unknown');
    expect(publication(snap([br('f', true)], ['origin/main']), 'origin')).toBe('unpublished');
    expect(publication(snap([br('f', true, 'origin/f', 0)], ['origin/f']), 'origin')).toBe('published');
    expect(publication(snap([br('f', true, 'origin/f', 2)], ['origin/f']), 'origin')).toBe('ahead');
    // upstream configurado pero la rama remota ya no existe
    expect(publication(snap([br('f', true, 'origin/f', 0)], ['origin/main']), 'origin')).toBe('unpublished');
  });

  it('clasifica errores y los segundos de espera', () => {
    expect(errorKind(new ApiError({ code: 'AUTH', message: 'x' }))).toBe('auth');
    expect(errorKind(new ApiError({ code: 'RATE_LIMITED', message: 'x' }))).toBe('rateLimited');
    expect(errorKind(new ApiError({ code: 'IO', message: 'x' }))).toBe('other');
    expect(errorKind(new Error('boom'))).toBe('other');
    const rl = (detail: string | null) => new ApiError({ code: 'RATE_LIMITED', message: 'x', detail });
    expect(retrySeconds(rl('30'))).toBe(30);
    expect(retrySeconds(rl(null))).toBe(5);
    expect(retrySeconds(rl('0'))).toBe(5);
    expect(retrySeconds(rl('9999'))).toBe(120);
  });
});
