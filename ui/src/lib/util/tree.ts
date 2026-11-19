import type { StatusEntry } from '$lib/api/types';
import { pathText } from './gitpath';

export type ChangeRow<S extends string = string> =
  | { type: 'dir'; section: S; key: string; name: string; path: string; depth: number; entries: StatusEntry[]; open: boolean }
  | { type: 'file'; section: S; entry: StatusEntry; depth: number };

interface Node {
  name: string;
  path: string;
  children: Map<string, Node>;
  /** Ficheros de esta carpeta, en el orden en que llegan del estado de git. */
  files: { entry: StatusEntry; order: number }[];
  /** Todo el subárbol (para el recuento y las acciones por carpeta). */
  all: StatusEntry[];
  /** Posición del primer fichero del subárbol en la lista de origen. */
  first: number;
}

function newNode(name: string, path: string, first: number): Node {
  return { name, path, children: new Map(), files: [], all: [], first };
}

/**
 * Convierte los archivos de una sección en filas de árbol por carpetas: **cada nivel de la ruta es
 * un nodo propio** (`ui` › `src` › `lib` › `util` › `branchTree.ts`), sin compactar cadenas de
 * carpetas, para que se vea de qué carpeta cuelga cada fichero.
 *
 * El orden de los ficheros es el mismo que en modo lista: las carpetas y los ficheros de cada nivel
 * se intercalan por la posición del primer fichero de cada uno en la lista de origen.
 *
 * `closed` contiene las claves `sección:ruta` de las carpetas plegadas.
 */
export function buildTreeRows<S extends string>(section: S, entries: readonly StatusEntry[], closed: ReadonlySet<string>): ChangeRow<S>[] {
  const root = newNode('', '', Number.POSITIVE_INFINITY);
  entries.forEach((entry, order) => {
    const parts = pathText(entry.path).split('/');
    parts.pop();
    let node = root;
    node.all.push(entry);
    node.first = Math.min(node.first, order);
    let path = '';
    for (const part of parts) {
      // Una ruta con barra final no puede dejar un nivel vacío: se descarta.
      if (part === '') continue;
      path = path ? `${path}/${part}` : part;
      let child = node.children.get(part);
      if (!child) {
        child = newNode(part, path, order);
        node.children.set(part, child);
      }
      child.all.push(entry);
      child.first = Math.min(child.first, order);
      node = child;
    }
    node.files.push({ entry, order });
  });

  const rows: ChangeRow<S>[] = [];
  const walk = (node: Node, depth: number) => {
    const items: { order: number; dir?: Node; file?: StatusEntry }[] = [];
    for (const child of node.children.values()) items.push({ order: child.first, dir: child });
    for (const f of node.files) items.push({ order: f.order, file: f.entry });
    items.sort((a, b) => a.order - b.order);
    for (const item of items) {
      if (item.dir) {
        const key = `${section}:${item.dir.path}`;
        const open = !closed.has(key);
        rows.push({
          type: 'dir',
          section,
          key,
          name: item.dir.name,
          path: item.dir.path,
          depth,
          entries: item.dir.all,
          open
        });
        if (open) walk(item.dir, depth + 1);
      } else if (item.file) {
        rows.push({ type: 'file', section, entry: item.file, depth });
      }
    }
  };
  walk(root, 0);
  return rows;
}
