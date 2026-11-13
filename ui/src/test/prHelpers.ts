import type { MockOptions } from '$lib/api/mockApi';
import { prOf } from '$lib/stores/pullRequests.svelte';
import { loadedTab } from './helpers';

export const GH = 'git@github.com:acme/app.git';
export const AZ = 'https://dev.azure.com/org/proj/_git/app';

/**
 * Pestaña del repo simulado con `origin` apuntando a `url` (GitHub por omisión) y el soporte de PR
 * ya consultado. El mock de PR usa el proveedor del remoto de la pestaña.
 */
export async function prTab(opts: MockOptions & { url?: string; tabId?: string } = {}) {
  const { url = GH, tabId = 'tab-ariadna', ...mock } = opts;
  const ctx = await loadedTab(tabId, mock);
  const repo = ctx.api.__repos.get(tabId);
  if (repo && url) repo.remotes = [{ name: 'origin', fetchUrl: url, pushUrl: url }];
  const pr = prOf(ctx.tab);
  await pr.loadSupport();
  return { ...ctx, pr };
}
