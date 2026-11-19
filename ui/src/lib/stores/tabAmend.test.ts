import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AmendDraft } from '$lib/api/types';
import { loadedTab, resetGlobals } from '../../test/helpers';

beforeEach(resetGlobals);
afterEach(() => {
  resetGlobals();
  vi.restoreAllMocks();
});

const ultimoCommit: AmendDraft = {
  message: 'Anterior asunto\n\nAnterior cuerpo',
  canAmend: true,
  reason: null
};

/**
 * Activar «corregir último commit» lee el último commit de forma asíncrona. Si mientras tanto el
 * usuario desactiva la casilla (o cambia de repositorio), esa lectura llega tarde y no debe rellenar
 * nada: el siguiente commit normal saldría con el mensaje del anterior.
 */
describe('amend sin carreras', () => {
  /** Deja `amendDraft` en el aire y devuelve cómo resolverlo cuando convenga. */
  function lecturaLenta(api: { amendDraft: (id: string) => Promise<AmendDraft> }) {
    let resolver: (v: AmendDraft) => void = () => {};
    const espia = vi.spyOn(api, 'amendDraft').mockImplementation(
      () => new Promise<AmendDraft>((res) => (resolver = res))
    );
    return { espia, resolver: (v: AmendDraft) => resolver(v) };
  }

  it('descarta la lectura si el amend se desactiva antes de que resuelva', async () => {
    const { api, tab } = await loadedTab();
    tab.setFields('borrador mío', '');
    const { resolver } = lecturaLenta(api);

    const activando = tab.setAmend(true);
    await tab.setAmend(false);
    resolver(ultimoCommit);
    await activando;

    expect(tab.amend).toBe(false);
    expect(tab.summary).toBe('borrador mío');
    expect(tab.message).toBe('borrador mío');
  });

  it('descarta la lectura si cambia el repositorio antes de que resuelva', async () => {
    const { api, tab } = await loadedTab();
    tab.setFields('', '');
    const { resolver } = lecturaLenta(api);

    const activando = tab.setAmend(true);
    // El mismo tabId pasa a apuntar a otra carpeta: es otro repositorio.
    tab.updateInfo({ ...tab.info, path: '/repos/otro', name: 'otro' });
    resolver(ultimoCommit);
    await activando;

    expect(tab.amend).toBe(false);
    expect(tab.message).toBe('');
  });

  it('solo aplica la última activación cuando se enciende y apaga varias veces', async () => {
    const { api, tab } = await loadedTab();
    const pendientes: ((v: AmendDraft) => void)[] = [];
    vi.spyOn(api, 'amendDraft').mockImplementation(
      () => new Promise<AmendDraft>((res) => pendientes.push(res))
    );

    const primera = tab.setAmend(true);
    const segunda = tab.setAmend(true);

    // La primera lectura llega tarde: no debe pisar la segunda activación.
    pendientes[0]({ message: 'vieja', canAmend: true, reason: null });
    await primera;
    expect(tab.message).toBe('');

    pendientes[1](ultimoCommit);
    await segunda;
    expect(tab.amend).toBe(true);
    expect(tab.message).toBe(ultimoCommit.message);
  });

  it('desactivar restaura el borrador que había antes de activar', async () => {
    const { api, tab } = await loadedTab();
    vi.spyOn(api, 'amendDraft').mockResolvedValue(ultimoCommit);
    tab.setFields('mío', 'cuerpo mío');

    await tab.setAmend(true);
    expect(tab.message).toBe(ultimoCommit.message);

    await tab.setAmend(false);
    expect(tab.amend).toBe(false);
    expect(tab.summary).toBe('mío');
    expect(tab.description).toBe('cuerpo mío');
  });
});
