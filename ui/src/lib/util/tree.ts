import type { StatusEntry } from '$lib/api/types';
import { pathText } from './gitpath';

export type ChangeRow<S extends string = string> =
  | { type: 'dir'; section: S; key: string; name: string; depth: number; entries: StatusEntry[]; open: boolean }
  | { type: 'file'; section: S; entry: StatusEntry; depth: number };

interface Node {
  name: string;
  children: Map<string, Node>;
  files: StatusEntry[];
  all: StatusEntry[];
}

function newNode(name: string): Node {
  return { name, children: new Map(), files: [], all: [] };
}

/**
 * Convierte los archivos de una sección en filas de árbol por carpetas. Las cadenas de
 * carpetas con un solo hijo se compactan («src/lib/api»). `closed` contiene las claves
 * `sección:ruta` de carpetas plegadas.
 */
export function buildTreeRows<S extends string>(section: S, entries: readonly StatusEntry[], closed: ReadonlySet<string>): ChangeRow<S>[] {
  const root = newNode('');
  for (const e of entries) {
    const parts = pathText(e.path).split('/');
    parts.pop();
    let n = root;
    n.all.push(e);
    for (const p of parts) {
      let c = n.children.get(p);
      if (!c) {
        c = newNode(p);
        n.children.set(p, c);
      }
      c.all.push(e);
      n = c;
    }
    n.files.push(e);
  }
  const rows: ChangeRow<S>[] = [];
  const walk = (node: Node, prefix: string, depth: number) => {
    const dirs = [...node.children.values()].sort((a, b) => a.name.localeCompare(b.name));
    for (const child of dirs) {
      let cur = child;
      let name = child.name;
      let path = prefix ? `${prefix}/${child.name}` : child.name;
      while (cur.files.length === 0 && cur.children.size === 1) {
        const only = [...cur.children.values()][0] as Node;
        name += `/${only.name}`;
        path += `/${only.name}`;
        cur = only;
      }
      const key = `${section}:${path}`;
      const open = !closed.has(key);
      rows.push({ type: 'dir', section, key, name, depth, entries: cur.all, open });
      if (open) walk(cur, path, depth + 1);
    }
    const files = [...node.files].sort((a, b) => pathText(a.path).localeCompare(pathText(b.path)));
    for (const entry of files) rows.push({ type: 'file', section, entry, depth });
  };
  walk(root, '', 0);
  return rows;
}
