import { describe, expect, it } from 'vitest';
import type { Commit } from '$lib/api/types';
import { Frontier, MAX_CURSOR } from './cursor';
import { tagNameProblem } from './tagName';

function c(sha: string, parents: string[] = []): Commit {
  return { sha, parents, subject: sha, body: '', authorName: 'a', authorEmail: 'a@x', authorDateIso: '2026-01-01T00:00:00Z', refs: [] };
}

/** Referencia: lo mismo que `ariadna_git::history_cursor`, recorriendo todo. */
function reference(shown: Commit[]): string[] {
  const loaded = new Set(shown.map((x) => x.sha));
  const seen = new Set<string>();
  const out: string[] = [];
  for (const x of shown) for (const p of x.parents) if (!loaded.has(p) && !seen.has(p)) (seen.add(p), out.push(p));
  return out;
}

describe('Frontier (cursor del historial)', () => {
  it('en un historial lineal es el padre del último commit', () => {
    const f = new Frontier();
    f.add([c('c', ['b']), c('b', ['a'])]);
    expect(f.list()).toEqual(['a']);
    f.add([c('a')]);
    expect(f.list()).toEqual([]);
    expect(f.usable).toBe(false);
  });

  it('con merges conserva las ramas pendientes, en el orden en que se encolaron', () => {
    const f = new Frontier();
    f.add([c('m', ['x', 'y']), c('x', ['r'])]);
    expect(f.list()).toEqual(['y', 'r']);
    f.add([c('y', ['r'])]);
    expect(f.list()).toEqual(['r']);
  });

  it('un padre que llega más tarde en la misma página no queda pendiente', () => {
    const f = new Frontier();
    f.add([c('m', ['x', 'y']), c('y', ['r']), c('x', ['r'])]);
    expect(f.list()).toEqual(['r']);
    expect(f.has('x') && f.has('y') && f.has('m') && !f.has('r')).toBe(true);
  });

  it('coincide con la referencia al añadir página a página en un grafo con merges', () => {
    // Grafo determinista: cada commit cuelga del anterior y, a veces, de uno más antiguo.
    const all: Commit[] = [];
    const n = 400;
    for (let i = n; i >= 1; i--) {
      const parents: string[] = [];
      if (i > 1) parents.push(`s${i - 1}`);
      if (i % 5 === 0 && i > 10) parents.push(`s${i - 7}`);
      all.push(c(`s${i}`, parents));
    }
    for (const size of [1, 3, 17, 100]) {
      const f = new Frontier();
      let shown: Commit[] = [];
      for (let k = 0; k < all.length; k += size) {
        const page = all.slice(k, k + size);
        f.add(page);
        shown = [...shown, ...page];
        expect(f.list()).toEqual(reference(shown));
      }
      expect(f.size).toBe(0);
    }
  });

  it('deja de ser utilizable si supera el máximo que acepta Rust', () => {
    const f = new Frontier();
    f.add([c('big', Array.from({ length: MAX_CURSOR }, (_, i) => `p${i}`))]);
    expect(f.usable).toBe(true);
    f.add([c('big2', ['extra'])]);
    expect(f.size).toBe(MAX_CURSOR + 1);
    expect(f.usable).toBe(false);
    f.reset();
    expect(f.size).toBe(0);
    expect(f.has('big')).toBe(false);
  });
});

describe('tagNameProblem', () => {
  it('acepta nombres normales, con barra y Unicode', () => {
    for (const ok of ['v1.0.0', 'release/1.0', '1.2.3-rc.1', 'ñandú', 'a@b']) expect(tagNameProblem(ok), ok).toBeNull();
  });

  it('rechaza lo que git check-ref-format rechaza', () => {
    const bad: [string, string][] = [
      ['', 'tags.problem.empty'],
      ['HEAD', 'tags.problem.head'],
      ['con espacio', 'tags.problem.spaces'],
      ['-x', 'tags.problem.dash'],
      ['a..b', 'tags.problem.chars'],
      ['a~1', 'tags.problem.chars'],
      ['a^', 'tags.problem.chars'],
      ['a:b', 'tags.problem.chars'],
      ['a?', 'tags.problem.chars'],
      ['a*', 'tags.problem.chars'],
      ['a[', 'tags.problem.chars'],
      ['a\\b', 'tags.problem.chars'],
      ['a@{1}', 'tags.problem.chars'],
      ['@', 'tags.problem.chars'],
      ['/a', 'tags.problem.slash'],
      ['a/', 'tags.problem.slash'],
      ['a//b', 'tags.problem.slash'],
      ['a.', 'tags.problem.dot'],
      ['.a', 'tags.problem.dot'],
      ['x/.y', 'tags.problem.dot'],
      ['v1.lock', 'tags.problem.dot'],
      ['x'.repeat(256), 'tags.problem.long']
    ];
    for (const [name, key] of bad) expect(tagNameProblem(name), name).toBe(key);
  });
});
