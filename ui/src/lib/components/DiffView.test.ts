import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '$lib/api/types';
import { toasts } from '$lib/stores/toasts.svelte';
import type { StatusEntry } from '$lib/api/types';
import type { SectionId } from '$lib/util/changes';
import type { TabState } from '$lib/stores/tab.svelte';
import { loadedTab, resetGlobals } from '../../test/helpers';
import DiffView from './DiffView.svelte';

beforeEach(resetGlobals);
afterEach(resetGlobals);

function pick(tab: TabState, section: SectionId, path: string): StatusEntry {
  const e = tab.sections[section].find((x) => x.path === path);
  if (!e) throw new Error(`${path} no está en ${section}`);
  tab.select(section, e);
  return e;
}

describe('DiffView', () => {
  it('sin selección muestra el estado vacío', async () => {
    const { tab } = await loadedTab();
    tab.selected = null;
    render(DiffView, { props: { tab } });
    expect(screen.getByText('Selecciona un archivo')).toBeInTheDocument();
  });

  it('pinta hunks con números de línea y un botón Preparar por hunk', async () => {
    const { tab } = await loadedTab();
    pick(tab, 'unstaged', 'src/lib/api/types.ts');
    render(DiffView, { props: { tab } });
    expect(await screen.findByText(/^@@ -14,\d+ \+14,\d+ @@$/)).toBeInTheDocument();
    const buttons = screen.getAllByRole('button', { name: /^Preparar el hunk \d de 2 de src\/lib\/api\/types\.ts$/ });
    expect(buttons).toHaveLength(2);
    // Adiciones y borrados diferenciados para lectores de pantalla
    expect(screen.getAllByText(/^Añadida:/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/^Eliminada:/).length).toBeGreaterThan(0);
  });

  it('preparar un hunk lo mueve a Preparados sin tocar el otro', async () => {
    const { tab, api } = await loadedTab();
    const entry = pick(tab, 'unstaged', 'src/lib/api/types.ts');
    render(DiffView, { props: { tab } });
    await userEvent.click((await screen.findAllByRole('button', { name: /^Preparar el hunk 1 de 2/ }))[0] as HTMLElement);
    await waitFor(async () => {
      const unstaged = await api.diff(tab.id, 'unstaged', [entry.path]);
      expect(unstaged[0]!.hunks).toHaveLength(1);
    });
    const staged = await api.diff(tab.id, 'staged', [entry.path]);
    expect(staged[0]!.hunks).toHaveLength(2); // el que ya había + el nuevo
    // El diff se recarga con el snapshot y queda 1 hunk sin preparar
    await waitFor(() => expect(screen.getAllByRole('button', { name: /^Preparar el hunk/ })).toHaveLength(1));
  });

  it('en el diff de preparados el botón es Quitar hunk', async () => {
    const { tab } = await loadedTab();
    pick(tab, 'staged', 'crates/ariadna-git/src/models.rs');
    render(DiffView, { props: { tab } });
    const btn = await screen.findByRole('button', { name: /^Quitar el hunk 1 de 1/ });
    expect(screen.getByText('Preparado')).toBeInTheDocument();
    await userEvent.click(btn);
    await waitFor(() => expect(tab.sections.staged.some((e) => e.path === 'crates/ariadna-git/src/models.rs')).toBe(false));
  });

  it('un hunk que ya no existe (HUNK_NOT_FOUND) se avisa y fuerza un refresco', async () => {
    const { tab, api } = await loadedTab();
    const entry = pick(tab, 'unstaged', 'src/lib/api/types.ts');
    render(DiffView, { props: { tab } });
    const btns = await screen.findAllByRole('button', { name: /^Preparar el hunk 1 de 2/ });
    // Otro proceso ya lo preparó: el backend responde HUNK_NOT_FOUND
    const refresh = vi.spyOn(api, 'refresh');
    api.stageHunk = async () => {
      throw new ApiError({ code: 'HUNK_NOT_FOUND', message: 'El hunk ya no existe.' });
    };
    await userEvent.click(btns[0] as HTMLElement);
    await waitFor(() => expect(toasts.items.some((t) => t.title === 'No se pudo preparar el hunk')).toBe(true));
    expect(toasts.items[0]!.message).toBe('El hunk ya no existe.');
    await waitFor(() => expect(refresh).toHaveBeenCalled());
    expect(entry.path).toBe('src/lib/api/types.ts');
  });

  it('los archivos binarios no se pintan', async () => {
    const { tab } = await loadedTab();
    pick(tab, 'unstaged', 'docs/screenshots/staging.png');
    render(DiffView, { props: { tab } });
    expect(await screen.findByText('Archivo binario')).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: /^Código de/ })).not.toBeInTheDocument();
  });

  it('un diff gigante (> 5 000 líneas) queda colapsado hasta que se pide', async () => {
    const { tab } = await loadedTab();
    pick(tab, 'unstaged', 'scratch/big-fixture.json');
    render(DiffView, { props: { tab } });
    expect(await screen.findByText('Diff muy grande')).toBeInTheDocument();
    expect(screen.getByText(/6\.200 líneas|6200 líneas/)).toBeInTheDocument();
    expect(document.querySelectorAll('.line')).toHaveLength(0);
    await userEvent.click(screen.getByRole('button', { name: 'Cargar diff igualmente' }));
    expect(await screen.findByRole('region', { name: /^Código de scratch/ })).toBeInTheDocument();
    // Virtualizado: no se montan las 6 200 líneas
    const lines = document.querySelectorAll('.line').length;
    expect(lines).toBeGreaterThan(10);
    expect(lines).toBeLessThan(150);
  });

  it('las líneas largas se truncan con «ver más» y se expanden', async () => {
    const { tab } = await loadedTab();
    pick(tab, 'unstaged', 'web/minified.js');
    render(DiffView, { props: { tab } });
    const more = await screen.findByRole('button', { name: /ver más/ });
    const textBefore = more.parentElement!.querySelector('.text')!.textContent!;
    expect(textBefore.length).toBe(500);
    await userEvent.click(more);
    await waitFor(() => expect(screen.queryByRole('button', { name: /ver más/ })).not.toBeInTheDocument());
    const expanded = [...document.querySelectorAll('.line .text')].some((n) => (n.textContent ?? '').length > 500);
    expect(expanded).toBe(true);
  });

  it('conserva tabuladores reales en el código', async () => {
    const { tab } = await loadedTab();
    pick(tab, 'unstaged', 'Makefile');
    render(DiffView, { props: { tab } });
    await screen.findByRole('region', { name: /^Código de Makefile/ });
    const texts = [...document.querySelectorAll('.line .text')].map((n) => n.textContent ?? '');
    expect(texts.some((t) => t.startsWith('\t'))).toBe(true);
  });

  it('un error al cargar muestra aviso con reintento', async () => {
    const { tab, api } = await loadedTab();
    pick(tab, 'unstaged', 'docs/ipc.md');
    api.diff = async () => {
      throw Object.assign(new Error('x'), { code: 'IO', message: 'No se pudo leer el archivo', detail: 'os error 5' });
    };
    render(DiffView, { props: { tab } });
    expect(await screen.findByText('No se pudo cargar el diff')).toBeInTheDocument();
    expect(screen.getByText('No se pudo leer el archivo')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Reintentar' })).toBeInTheDocument();
  });

  it('un archivo en conflicto ofrece abrirlo en el estudio de conflictos', async () => {
    const { tab } = await loadedTab('tab-legacy');
    pick(tab, 'conflicts', 'src/billing/invoice.ts');
    render(DiffView, { props: { tab } });
    expect(screen.getByText('Archivo en conflicto')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Abrir en Conflictos' }));
    expect(tab.panel).toBe('conflicts');
  });
});
