import type { MessageKey } from '$lib/i18n';

/**
 * Comprobación ligera del nombre de una rama (reglas de `git check-ref-format`) para dar
 * feedback inmediato. Rust valida de verdad (`INVALID_REFERENCE`); esto solo ahorra un viaje.
 * Devuelve la clave i18n del problema o `null` si el nombre parece válido.
 */
export function branchNameProblem(name: string): MessageKey | null {
  if (name.trim() === '') return 'branches.problem.empty';
  if (name !== name.trim() || /\s/.test(name)) return 'branches.problem.spaces';
  if (name.startsWith('-')) return 'branches.problem.dash';
  // eslint-disable-next-line no-control-regex
  if (/[~^:?*[\\\u0000-\u001f\u007f]/.test(name)) return 'branches.problem.chars';
  if (name.includes('..') || name.includes('@{') || name === '@') return 'branches.problem.chars';
  if (name.startsWith('/') || name.endsWith('/') || name.includes('//')) return 'branches.problem.slash';
  if (name.endsWith('.') || name.endsWith('.lock') || name.split('/').some((p) => p.startsWith('.'))) return 'branches.problem.dot';
  return null;
}
