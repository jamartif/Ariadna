import type { GitPath } from '$lib/api/types';

/** Clave estable para usar un `GitPath` como clave de mapa o de selección. */
export function pathKey(p: GitPath): string {
  return typeof p === 'string' ? p : `bytes:${p.join(',')}`;
}

export function pathEq(a: GitPath, b: GitPath): boolean {
  return pathKey(a) === pathKey(b);
}

const decoder = new TextDecoder('utf-8', { fatal: false });

/** Texto para mostrar. Los bytes no UTF-8 se decodifican con sustitución. */
export function pathText(p: GitPath): string {
  return typeof p === 'string' ? p : decoder.decode(new Uint8Array(p));
}

export function pathBase(p: GitPath): string {
  const t = pathText(p);
  const i = t.lastIndexOf('/');
  return i < 0 ? t : t.slice(i + 1);
}

export function pathDir(p: GitPath): string {
  const t = pathText(p);
  const i = t.lastIndexOf('/');
  return i < 0 ? '' : t.slice(0, i);
}
