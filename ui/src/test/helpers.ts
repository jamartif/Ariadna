import { waitFor } from '@testing-library/svelte';
import { createMockApi, type MockOptions } from '$lib/api/mockApi';
import { AppStore } from '$lib/stores/app.svelte';
import { TabState } from '$lib/stores/tab.svelte';
import { settings } from '$lib/stores/settings.svelte';
import { toasts } from '$lib/stores/toasts.svelte';

export function mockApi(opts: MockOptions = {}) {
  return createMockApi({ latency: 0, confirm: () => true, ...opts });
}

/** Crea y carga el estado de una pestaña del repo simulado (`ariadna` por defecto). */
export async function loadedTab(tabId = 'tab-ariadna', opts: MockOptions = {}) {
  const api = mockApi(opts);
  const state = await api.appState();
  const info = state.tabs.find((t) => t.tabId === tabId);
  if (!info) throw new Error(`pestaña ${tabId} no existe en el mock`);
  const tab = new TabState(api, info);
  await tab.attach();
  await tab.load();
  return { api, tab };
}

export async function readyApp(opts: MockOptions = {}) {
  const api = mockApi(opts);
  const app = new AppStore(api);
  await app.init();
  return { api, app };
}

export function resetGlobals() {
  toasts.clear();
  localStorage.clear();
  settings.theme = 'system';
  settings.density = 'comfortable';
  document.documentElement.removeAttribute('data-theme');
  document.documentElement.removeAttribute('data-density');
}

export { waitFor };
