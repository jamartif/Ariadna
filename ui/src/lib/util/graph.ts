/**
 * Grafo simple de ramas para el historial. Función pura y **incremental**: se alimenta de
 * `git log` (del más nuevo al más viejo, hijos antes que padres) y se puede seguir
 * ampliando con la página siguiente conservando el `GraphState`.
 *
 * Cada fila describe cómo dibujar una celda de altura fija:
 *  - `lane`: columna del nodo del commit;
 *  - `through`: columnas con una línea vertical que atraviesa la fila entera;
 *  - `joins`: columnas (≠ `lane`) cuyo hilo llega desde arriba y termina en el nodo (ramas que
 *    se unen aquí: el commit es su padre);
 *  - `parents`: columnas hacia las que sale una línea desde el nodo hacia abajo (una por padre;
 *    la primera es la continuación de la rama, las demás son los padres de un merge);
 *  - `incoming`: ¿entra una línea desde arriba al nodo? (falso en la punta de una rama).
 */

export interface GraphRow {
  lane: number;
  incoming: boolean;
  through: number[];
  joins: number[];
  parents: number[];
  /** Número de columnas ocupadas en esta fila (para dimensionar el SVG). */
  width: number;
}

export interface GraphState {
  /** Por columna: el hash que se espera a continuación en ella, o `null` si está libre. */
  lanes: (string | null)[];
}

export const emptyGraphState = (): GraphState => ({ lanes: [] });

export interface GraphCommit {
  sha: string;
  parents: string[];
}

function firstFree(lanes: (string | null)[], skip: ReadonlySet<number> = new Set()): number {
  for (let i = 0; i < lanes.length; i++) if (lanes[i] === null && !skip.has(i)) return i;
  lanes.push(null);
  return lanes.length - 1;
}

/**
 * Calcula las filas de `commits` a partir de `state` (que se modifica y se devuelve para
 * poder continuar con la página siguiente).
 */
export function layoutGraph(commits: readonly GraphCommit[], state: GraphState = emptyGraphState()): { rows: GraphRow[]; state: GraphState } {
  const lanes = state.lanes;
  const rows: GraphRow[] = [];
  for (const c of commits) {
    const matches: number[] = [];
    lanes.forEach((s, i) => {
      if (s === c.sha) matches.push(i);
    });
    const incoming = matches.length > 0;
    const lane = incoming ? (matches[0] as number) : firstFree(lanes);
    const joins = matches.filter((m) => m !== lane);

    // Columnas que atraviesan la fila sin tocar el nodo.
    const through: number[] = [];
    lanes.forEach((s, i) => {
      if (s !== null && i !== lane && !joins.includes(i)) through.push(i);
    });
    for (const m of matches) lanes[m] = null;

    const parentLanes: number[] = [];
    const [p0, ...rest] = c.parents;
    if (p0 !== undefined) {
      // La continuación de la rama conserva la columna del nodo, salvo que otro hilo ya espere a p0.
      const existing = lanes.indexOf(p0);
      if (existing >= 0) parentLanes.push(existing);
      else {
        lanes[lane] = p0;
        parentLanes.push(lane);
      }
    }
    for (const p of rest) {
      const existing = lanes.indexOf(p);
      if (existing >= 0) {
        parentLanes.push(existing);
      } else {
        const l = firstFree(lanes, new Set([lane]));
        lanes[l] = p;
        parentLanes.push(l);
      }
    }
    // Las columnas libres del final no ocupan anchura.
    while (lanes.length > 0 && lanes[lanes.length - 1] === null) lanes.pop();
    const width = Math.max(lane, ...through, ...joins, ...parentLanes, 0) + 1;
    rows.push({ lane, incoming, through, joins, parents: parentLanes, width });
  }
  return { rows, state: { lanes } };
}

/** Anchura máxima en columnas de un conjunto de filas. */
export function maxWidth(rows: readonly GraphRow[]): number {
  let w = 1;
  for (const r of rows) if (r.width > w) w = r.width;
  return w;
}
