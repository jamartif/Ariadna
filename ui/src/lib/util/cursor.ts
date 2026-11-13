import type { Commit } from '$lib/api/types';

/** Máximo de commits en una frontera que Rust acepta (`MAX_CURSOR`). Más → se pagina con `skip`. */
export const MAX_CURSOR = 1000;

/**
 * Frontera del historial cargado: los commits que son padres de algo ya cargado y todavía no se
 * han cargado, en el orden en que `git log` los fue encolando. Es el cursor de `logPage`:
 * Rust continúa el recorrido desde ahí (`git log <frontera>`), sin `--skip`, y el coste no
 * depende de lo hondo que esté la página. Equivale a `ariadna_git::history_cursor` pero es
 * incremental (se actualiza con cada página en vez de recorrer todo).
 *
 * En un historial lineal es solo el padre del último commit. Con merges son varios: no basta
 * con el último hash porque se perderían las ramas aún pendientes (ver `specs/rust/rendimiento.md`).
 */
export class Frontier {
  private loaded = new Set<string>();
  /** `Map` conserva el orden de inserción. */
  private pending = new Map<string, true>();

  get size(): number {
    return this.pending.size;
  }

  /** `true` si ya se cargó ese commit. */
  has(sha: string): boolean {
    return this.loaded.has(sha);
  }

  /** Hashes desde los que continuar, en orden. */
  list(): string[] {
    return [...this.pending.keys()];
  }

  /** `true` si cabe en un cursor (y no está vacío). */
  get usable(): boolean {
    return this.pending.size > 0 && this.pending.size <= MAX_CURSOR;
  }

  /** Incorpora commits, en el orden de la página. */
  add(commits: readonly Commit[]): void {
    for (const c of commits) {
      this.loaded.add(c.sha);
      this.pending.delete(c.sha);
      for (const p of c.parents) {
        if (!this.loaded.has(p) && !this.pending.has(p)) this.pending.set(p, true);
      }
    }
  }

  reset(): void {
    this.loaded.clear();
    this.pending.clear();
  }
}
