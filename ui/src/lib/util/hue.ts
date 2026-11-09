/** Matiz estable (0–359) a partir del nombre de un repo: da a cada uno su punto de color. */
export function repoHue(name: string): number {
  let h = 2166136261;
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  // Evita el rango del acento (cian) y el de los semánticos rojo/ámbar para no confundir significados.
  const palette = [262, 292, 330, 210, 168, 28, 140, 238, 190, 350];
  return palette[Math.abs(h) % palette.length] as number;
}
