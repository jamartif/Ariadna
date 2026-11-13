import { describe, expect, it } from 'vitest';
import type { ProfileRule } from '$lib/api/types';
import { matchRule, normalizePattern, remoteSegments } from './profileRules';

const rules: ProfileRule[] = [
  { pattern: 'dev.azure.com', profile: 'empresa' },
  { pattern: 'github.com', profile: 'personal' },
  { pattern: 'github.com/mi-empresa', profile: 'empresa' }
];

describe('reglas repo → perfil (misma lógica que Rust)', () => {
  it('descompone https, ssh y scp, incluido Azure DevOps SSH', () => {
    expect(remoteSegments('https://dev.azure.com/acme/proj/_git/api')).toEqual(['dev.azure.com', 'acme', 'proj', 'api']);
    expect(remoteSegments('git@ssh.dev.azure.com:v3/acme/proj/api')).toEqual(['dev.azure.com', 'acme', 'proj', 'api']);
    expect(remoteSegments('git@github.com:Javie/Ariadna.git')).toEqual(['github.com', 'javie', 'ariadna']);
    expect(remoteSegments('https://user@github.com/Org/Repo.git')).toEqual(['github.com', 'org', 'repo']);
    expect(remoteSegments('/ruta/local')).toEqual([]);
  });

  it('gana la regla más específica y se compara por segmentos', () => {
    expect(matchRule(rules, 'https://dev.azure.com/acme/p/_git/r')).toEqual({ profile: 'empresa', pattern: 'dev.azure.com' });
    expect(matchRule(rules, 'git@github.com:Mi-Empresa/api.git')).toEqual({ profile: 'empresa', pattern: 'github.com/mi-empresa' });
    expect(matchRule(rules, 'https://github.com/mi-empresa-2/x.git')).toEqual({ profile: 'personal', pattern: 'github.com' });
    expect(matchRule(rules, 'https://gitlab.com/a/b.git')).toBeNull();
    expect(matchRule(rules, '')).toBeNull();
  });

  it('normaliza y rechaza patrones', () => {
    expect(normalizePattern('  Dev.Azure.com/Mi-Org/ ')).toEqual({ ok: true, pattern: 'dev.azure.com/mi-org' });
    const why = (s: string) => {
      const r = normalizePattern(s);
      return r.ok ? 'ok' : r.reason;
    };
    expect(why('')).toBe('empty');
    expect(why('https://github.com')).toBe('scheme');
    expect(why('a@github.com')).toBe('scheme');
    expect(why('git hub.com')).toBe('chars');
    expect(why('github')).toBe('host');
    expect(why('.github.com')).toBe('host');
    expect(why('github.com//x')).toBe('segment');
    expect(why('a'.repeat(201))).toBe('long');
  });
});
