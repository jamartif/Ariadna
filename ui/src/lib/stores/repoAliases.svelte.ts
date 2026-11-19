import type { Api } from '$lib/api/api';
import type { AppSettings } from '$lib/api/types';

/**
 * Alias de repositorios: un nombre corto por ruta, guardado en la configuración (Rust) para que la
 * barra superior no enseñe rutas largas. Es **solo presentación**: la ruta completa sigue mandando
 * en todo lo demás (tooltip, comandos, pestañas), y sin alias se recorta el nombre como siempre.
 */
class RepoAliases {
  map = $state<Record<string, string>>({});

  /** Copia el mapa guardado en `settings.json` (al arrancar y después de cada guardado). */
  hydrate(settings: AppSettings): void {
    this.map = { ...(settings.repoAliases ?? {}) };
  }

  /** Alias de una ruta, o `null` si no tiene. */
  get(path: string): string | null {
    return this.map[path] ?? null;
  }

  /** Nombre visible de la pestaña: el alias si lo hay, si no el nombre del repo. */
  label(path: string, name: string): string {
    return this.get(path) ?? name;
  }

  /**
   * Guarda (o borra, con cadena vacía) el alias de un repo. Parte de los ajustes guardados para no
   * pisar lo que haya cambiado el panel de Ajustes mientras tanto.
   */
  async set(api: Api, path: string, alias: string): Promise<void> {
    const current = (await api.getSettings()).settings;
    const next: Record<string, string> = { ...(current.repoAliases ?? {}) };
    const value = alias.trim();
    if (value === '') delete next[path];
    else next[path] = value;
    const view = await api.saveSettings({ ...current, repoAliases: next });
    this.hydrate(view.settings);
  }
}

export const repoAliases = new RepoAliases();
