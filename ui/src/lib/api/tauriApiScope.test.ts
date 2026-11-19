import { describe, expect, it, vi } from 'vitest';

/**
 * El ámbito del historial tiene que viajar **siempre** en la petición: si se omite, el backend
 * antiguo se quedaba mirando solo HEAD y el grafo completo no aparecía.
 */
const llamadas: { command: string; args: Record<string, unknown> }[] = [];

vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (command: string, args: Record<string, unknown>) => {
    llamadas.push({ command, args });
    return [];
  }
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: async () => () => undefined
}));

const { createTauriApi } = await import('./tauriApi');

describe('log_page: el ámbito del historial va siempre en la petición', () => {
  it('sin ámbito explícito pide el grafo completo, no solo HEAD', async () => {
    const api = createTauriApi();
    llamadas.length = 0;
    await api.logPage('tab-1', 0, 100, null);
    expect(llamadas).toHaveLength(1);
    expect(llamadas[0]?.command).toBe('log_page');
    expect(llamadas[0]?.args.scope).toBe('all');
  });

  it('con ámbito explícito se respeta, también al paginar', async () => {
    const api = createTauriApi();
    llamadas.length = 0;
    await api.logPage('tab-1', 0, 100, null, null, 'current');
    await api.logPage('tab-1', 100, 100, null, null, 'current');
    expect(llamadas.map((l) => l.args.scope)).toEqual(['current', 'current']);
    expect(llamadas.map((l) => l.args.skip)).toEqual([0, 100]);
  });
});
