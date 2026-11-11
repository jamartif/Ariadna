import type { Api } from './api';
import { createMockApi } from './mockApi';
import { createTauriApi } from './tauriApi';

export type { Api, Unlisten } from './api';
export * from './types';
export * from './prTypes';
export type { PullRequestApi } from './prApi';

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * Tauri si `window.__TAURI_INTERNALS__` existe; si no, el backend simulado
 * (así `npm run dev` funciona en un navegador normal).
 * Parámetro `?scenario=` del mock: `empty`, `nogit`, `slow`, `error`.
 */
export function selectApi(): Api {
  if (isTauri()) return createTauriApi();
  const scenario =
    typeof location !== 'undefined' ? new URLSearchParams(location.search).get('scenario') : null;
  return createMockApi({ scenario: scenario ?? undefined });
}
