import type { AiProfileKind, AppSettings, ProfileRule } from '$lib/api/types';

/**
 * Misma lógica que `settings.rs` (`match_rule`), solo para el mock y para la vista previa de
 * la interfaz. La decisión real la toma Rust al generar con IA.
 */

/** Segmentos `host/propietario/…` (minúsculas) de la URL de un remoto. */
export function remoteSegments(url: string): string[] {
  const u = url.trim();
  let host = '';
  let path = '';
  const web = /^[a-z][a-z0-9+.-]*:\/\/(?:[^@/]*@)?([^/:]+)(?::\d+)?\/(.*)$/i.exec(u);
  const scp = /^(?:[^@/]+@)?([^/:]+):(?!\/\/)(.*)$/.exec(u);
  if (web) [host, path] = [web[1] ?? '', web[2] ?? ''];
  else if (scp) [host, path] = [scp[1] ?? '', scp[2] ?? ''];
  else return [];
  host = host.toLowerCase();
  if (host === 'ssh.dev.azure.com' || host === 'vs-ssh.visualstudio.com') host = 'dev.azure.com';
  const rest = path
    .replace(/\.git$/i, '')
    .split('/')
    .filter((s) => s !== '' && s !== '_git' && s !== 'v3')
    .map((s) => s.toLowerCase());
  return [host, ...rest];
}

export function matchRule(rules: readonly ProfileRule[], url: string): { profile: AiProfileKind; pattern: string } | null {
  const segs = remoteSegments(url);
  if (segs.length === 0) return null;
  let best: { n: number; r: ProfileRule } | null = null;
  for (const r of rules) {
    const ps = r.pattern.split('/');
    if (ps.length <= segs.length && ps.every((p, i) => p === segs[i]) && (!best || ps.length > best.n)) best = { n: ps.length, r };
  }
  return best ? { profile: best.r.profile, pattern: best.r.pattern } : null;
}

/** Normaliza un patrón como lo hace Rust; devuelve el motivo del rechazo o el patrón limpio. */
export function normalizePattern(raw: string): { ok: true; pattern: string } | { ok: false; reason: 'empty' | 'long' | 'scheme' | 'chars' | 'host' | 'segment' } {
  const p = raw.trim().replace(/^\/+|\/+$/g, '').toLowerCase();
  if (p === '') return { ok: false, reason: 'empty' };
  if (p.length > 200) return { ok: false, reason: 'long' };
  if (p.includes('://') || p.includes('@')) return { ok: false, reason: 'scheme' };
  if (!/^[a-z0-9._\-/~]+$/.test(p)) return { ok: false, reason: 'chars' };
  const [host = '', ...rest] = p.split('/');
  if (host === '' || host.startsWith('.') || host.endsWith('.') || host.startsWith('-') || (!host.includes('.') && host !== 'localhost')) return { ok: false, reason: 'host' };
  if (rest.some((s) => s === '')) return { ok: false, reason: 'segment' };
  return { ok: true, pattern: p };
}

export type SettingsDraft = Pick<AppSettings, 'profileRules'>;
