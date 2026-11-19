/**
 * Nombre visible de una rama dentro del árbol del riel.
 *
 * Con `origin/feature/rama` colgando ya de `origin/` y de `feature/`, la fila tiene que decir solo
 * `rama`: repetir los prefijos de los nodos padre sobra y come ancho. La referencia completa sigue
 * viviendo en `Branch.name` (tooltip, `aria-label`, menú contextual y acciones), así que esto es
 * **solo presentación**.
 *
 * Casos raros que no pueden dejar una fila vacía:
 * - un nombre que termina en `/` (`origin/feature/`): el último segmento es vacío, se usa el
 *   último con contenido (`feature`);
 * - un prefijo que no encaja (nombre inesperado): se devuelve el nombre completo.
 */
export function leafLabel(name: string, prefix: string): string {
  const rest = prefix !== '' && name.startsWith(prefix) ? name.slice(prefix.length) : name;
  if (rest !== '') return rest;
  const segs = name.split('/').filter((s) => s !== '');
  return segs[segs.length - 1] ?? name;
}
