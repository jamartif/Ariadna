import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { loadedTab, resetGlobals } from '../../test/helpers';
import CommitReportDialog from './CommitReportDialog.svelte';
import Toasts from './Toasts.svelte';

beforeEach(resetGlobals);
afterEach(() => {
  resetGlobals();
  vi.restoreAllMocks();
});


/**
 * El aviso de «contexto demasiado grande» tiene que apuntar al ajuste real («Tamaño de contexto»,
 * en Ajustes › IA) y ofrecer reintentar con más contexto sin cerrar el diálogo. Antes no existía ni
 * el ajuste ni el reintento: el aviso mandaba a Ajustes, donde no había nada.
 */
describe('CommitReportDialog · contexto demasiado grande', () => {
  async function setup() {
    const ctx = await loadedTab('tab-ariadna', { scenario: 'ai-too-large', confirm: () => true });
    const onclose = vi.fn();
    // El commit tiene que existir en el repo del mock: se toma el primero del historial.
    const page = await ctx.api.logPage('tab-ariadna', 0, 1, null, null, 'all');
    const real = page[0]!;
    render(CommitReportDialog, { props: { tab: ctx.tab, commit: real, onclose } });
    render(Toasts);
    await new Promise((r) => setTimeout(r, 80));
    return { ...ctx, onclose };
  }

  it('nombra el ajuste y ofrece reintentar con más contexto', async () => {
    const { tab } = await setup();

    const hint = await screen.findByTestId('report-context-hint');
    expect(hint.textContent).toContain('Ajustes › IA');
    expect(hint.textContent).toContain('Tamaño de contexto');
    expect(hint.textContent).toContain('100');

    const retry = await screen.findByTestId('report-more-context');
    expect(retry.textContent).toContain('Reintentar con');
    expect(tab.api).toBeTruthy();
  });

  it('sube el tope en los ajustes y reintenta sin cerrar el diálogo', async () => {
    const { tab, onclose } = await setup();
    const patch = vi.spyOn(tab.api, 'aiUpdateSettings');
    const report = vi.spyOn(tab.api, 'aiCommitReport');

    const retry = await screen.findByTestId('report-more-context');
    await userEvent.click(retry);

    await waitFor(() => expect(patch).toHaveBeenCalledTimes(1));
    await new Promise((r) => setTimeout(r, 30));
    // 100 000 → 200 000: se dobla el tope actual, que es el mínimo útil.
    expect(patch.mock.calls[0]![0]).toMatchObject({ maxContextBytes: 200_000 });
    // Y se vuelve a pedir el informe sin cerrar el diálogo.
    // El espía se instala después del montaje: 1 llamada = ya se volvió a pedir el informe.
    await waitFor(() => expect(report.mock.calls.length).toBeGreaterThanOrEqual(1), { timeout: 5000 });
    expect(onclose).not.toHaveBeenCalled();
  });
});
