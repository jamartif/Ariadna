import type { MessageKey } from '$lib/i18n';

/** Mismo tope que Rust (`MAX_TAG_NAME`). */
export const MAX_TAG_NAME = 255;

/**
 * Comprobación ligera del nombre de una etiqueta (reglas de `git check-ref-format`) para dar
 * feedback inmediato. Rust valida de verdad (`INVALID_TAG_NAME`); esto solo ahorra un viaje.
 * Devuelve la clave i18n del problema o `null` si el nombre parece válido.
 */
export function tagNameProblem(name: string): MessageKey | null {
  if (name === '') return 'tags.problem.empty';
  if (name.length > MAX_TAG_NAME) return 'tags.problem.long';
  if (name === 'HEAD') return 'tags.problem.head';
  if (/\s/.test(name)) return 'tags.problem.spaces';
  if (name.startsWith('-')) return 'tags.problem.dash';
  // eslint-disable-next-line no-control-regex
  if (/[~^:?*[\\\u0000-\u001f\u007f]/.test(name)) return 'tags.problem.chars';
  if (name.includes('..') || name.includes('@{') || name === '@') return 'tags.problem.chars';
  if (name.startsWith('/') || name.endsWith('/') || name.includes('//')) return 'tags.problem.slash';
  if (name.endsWith('.') || name.split('/').some((p) => p.startsWith('.') || p.endsWith('.lock'))) return 'tags.problem.dot';
  return null;
}
