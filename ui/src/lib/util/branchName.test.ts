import { describe, expect, it } from 'vitest';
import { branchNameProblem } from './branchName';

describe('nombre de rama', () => {
  it('acepta nombres normales', () => {
    for (const ok of ['main', 'feature/mi-cambio', 'fix_123', 'v1.2.0', 'ñandú', 'a/b/c']) expect(branchNameProblem(ok), ok).toBeNull();
  });
  it('rechaza lo que git rechaza', () => {
    const bad: [string, string][] = [
      ['', 'branches.problem.empty'],
      ['  ', 'branches.problem.empty'],
      ['con espacio', 'branches.problem.spaces'],
      [' x', 'branches.problem.spaces'],
      ['-rf', 'branches.problem.dash'],
      ['a..b', 'branches.problem.chars'],
      ['a~1', 'branches.problem.chars'],
      ['a^', 'branches.problem.chars'],
      ['a:b', 'branches.problem.chars'],
      ['a?b', 'branches.problem.chars'],
      ['a*b', 'branches.problem.chars'],
      ['a[b', 'branches.problem.chars'],
      ['a\\b', 'branches.problem.chars'],
      ['x@{y', 'branches.problem.chars'],
      ['@', 'branches.problem.chars'],
      ['/x', 'branches.problem.slash'],
      ['x/', 'branches.problem.slash'],
      ['x//y', 'branches.problem.slash'],
      ['x.', 'branches.problem.dot'],
      ['x.lock', 'branches.problem.dot'],
      ['.oculta', 'branches.problem.dot'],
      ['a/.b', 'branches.problem.dot']
    ];
    for (const [name, key] of bad) expect(branchNameProblem(name), JSON.stringify(name)).toBe(key);
  });
});
