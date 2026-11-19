import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { repoAliases } from './repoAliases.svelte';
import { loadedTab, resetGlobals } from '../../test/helpers';

beforeEach(() => {
  resetGlobals();
  repoAliases.map = {};
});
afterEach(resetGlobals);

const REPO = 'C:/Users/javie/dev/Ariadna';

/**
 * El alias es un campo de la configuración persistida: se guarda, se aplica al nombre visible y
 * sobrevive a recargar los ajustes (que es lo que pasa al reabrir la vista o reiniciar la app).
 */
describe('alias de repositorios', () => {
  it('se guarda en los ajustes y se aplica al nombre visible', async () => {
    const { api } = await loadedTab();
    expect(repoAliases.label(REPO, 'ariadna')).toBe('ariadna');

    await repoAliases.set(api, REPO, '  Aria  ');
    expect(repoAliases.get(REPO)).toBe('Aria');
    expect(repoAliases.label(REPO, 'ariadna')).toBe('Aria');

    // Persistido: está en settings.json (lo que devuelve el núcleo).
    const saved = (await api.getSettings()).settings;
    expect(saved.repoAliases[REPO]).toBe('Aria');
  });

  it('se recupera al volver a leer los ajustes (vista nueva)', async () => {
    const { api } = await loadedTab();
    await repoAliases.set(api, REPO, 'Aria');

    // Simula un arranque: el mapa en memoria se vacía y se hidrata con lo guardado.
    repoAliases.map = {};
    expect(repoAliases.get(REPO)).toBeNull();
    repoAliases.hydrate((await api.getSettings()).settings);
    expect(repoAliases.label(REPO, 'ariadna')).toBe('Aria');
  });

  it('valida longitud y caracteres, y el vacío borra el alias', async () => {
    const { api } = await loadedTab();
    await expect(repoAliases.set(api, REPO, 'x'.repeat(25))).rejects.toMatchObject({ code: 'PARSE' });
    await expect(repoAliases.set(api, REPO, 'mal😀nombre')).rejects.toMatchObject({ code: 'PARSE' });

    await repoAliases.set(api, REPO, 'Aria');
    expect(repoAliases.get(REPO)).toBe('Aria');
    await repoAliases.set(api, REPO, '   ');
    expect(repoAliases.get(REPO)).toBeNull();
    expect((await api.getSettings()).settings.repoAliases[REPO]).toBeUndefined();
  });
});
