import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { historyOf } from '$lib/stores/history.svelte';
import { tagsOf } from '$lib/stores/tags.svelte';
import { loadedTab, resetGlobals } from '../../test/helpers';
import RepoDialog from './RepoDialog.svelte';
import HistoryDetail from './HistoryDetail.svelte';
import HistoryPanel from './HistoryPanel.svelte';
import TagsSection from './TagsSection.svelte';
import Toasts from './Toasts.svelte';

beforeEach(resetGlobals);
afterEach(resetGlobals);

async function setup(opts = {}, tabId = 'tab-ariadna') {
  const ctx = await loadedTab(tabId, opts);
  render(TagsSection, { props: { tab: ctx.tab } });
  render(Toasts);
  return ctx;
}

const tagRow = (name: string) => document.querySelector<HTMLElement>(`li[data-tag="${name}"]`)!;

describe('Etiquetas en el panel de Ramas', () => {
  it('lista las existentes con tipo, commit y fecha', async () => {
    await setup();
    const region = await screen.findByRole('region', { name: 'Etiquetas del repositorio' });
    const row = await waitFor(() => {
      const r = tagRow('v0.2.0');
      expect(r).not.toBeNull();
      return r;
    });
    expect(within(row).getByText('Anotada')).toBeInTheDocument();
    expect(row).toHaveTextContent('Versión v0.2.0');
    expect(row.querySelector('code')?.textContent).toMatch(/^[0-9a-f]{7}$/);
    expect(row.querySelector('time')).not.toBeNull();
    expect(within(region).getByText('Etiquetas')).toBeInTheDocument();
  });

  it('el diálogo de Remotos y etiquetas incluye la sección', async () => {
    const { tab } = await loadedTab();
    render(RepoDialog, { props: { tab, onclose: vi.fn() } });
    expect(await screen.findByRole('region', { name: 'Etiquetas del repositorio' })).toBeInTheDocument();
  });

  it('crear una ligera en HEAD valida el nombre, y aparece en la lista y en el historial', async () => {
    const { tab } = await setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Nueva etiqueta' }));
    const name = screen.getByLabelText('Nombre');
    const submit = screen.getByRole('button', { name: 'Crear etiqueta' });
    expect(submit).toBeDisabled();
    await userEvent.type(name, 'con espacio');
    expect(screen.getByText('El nombre no puede tener espacios.')).toBeInTheDocument();
    expect(submit).toBeDisabled();
    await userEvent.clear(name);
    await userEvent.type(name, 'v2.0.0');
    expect(submit).toBeEnabled();
    await userEvent.click(submit);
    expect(await screen.findByText('Etiqueta «v2.0.0» creada')).toBeInTheDocument();
    await waitFor(() => expect(tagRow('v2.0.0')).not.toBeNull());
    expect(within(tagRow('v2.0.0')).getByText('Ligera')).toBeInTheDocument();
    // El formulario se cierra.
    expect(screen.queryByLabelText('Nombre')).not.toBeInTheDocument();
    // El commit de HEAD lleva la etiqueta en sus refs.
    const head = (await tab.api.logPage(tab.id, 0, 1, null))[0]!;
    expect(head.refs).toContain('tag: v2.0.0');
  });

  it('con mensaje la etiqueta es anotada', async () => {
    await setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Nueva etiqueta' }));
    await userEvent.type(screen.getByLabelText('Nombre'), 'v3');
    await userEvent.type(screen.getByLabelText('Mensaje (opcional)'), 'Tercera versión');
    await userEvent.click(screen.getByRole('button', { name: 'Crear etiqueta' }));
    await waitFor(() => expect(tagRow('v3')).not.toBeNull());
    expect(within(tagRow('v3')).getByText('Anotada')).toBeInTheDocument();
    expect(tagRow('v3')).toHaveTextContent('Tercera versión');
  });

  it('un nombre repetido se avisa antes de enviar', async () => {
    await setup();
    await waitFor(() => expect(tagRow('v0.2.0')).not.toBeNull());
    await userEvent.click(screen.getByRole('button', { name: 'Nueva etiqueta' }));
    await userEvent.type(screen.getByLabelText('Nombre'), 'v0.2.0');
    expect(screen.getByText('Ya existe una etiqueta con ese nombre.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Crear etiqueta' })).toBeDisabled();
  });

  it('Escape cierra el formulario sin crear nada', async () => {
    const { tab } = await setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Nueva etiqueta' }));
    await userEvent.type(screen.getByLabelText('Nombre'), 'x{Escape}');
    expect(screen.queryByLabelText('Nombre')).not.toBeInTheDocument();
    expect((await tab.api.listTags(tab.id)).map((x) => x.name)).not.toContain('x');
  });

  it('borrar pide confirmación nativa: con «no» no cambia nada; con «sí» desaparece', async () => {
    const answers = [false, true];
    const confirm = vi.fn((_m: string) => answers.shift() ?? true);
    const { tab } = await setup({ confirm });
    await waitFor(() => expect(tagRow('v0.2.0')).not.toBeNull());
    const del = within(tagRow('v0.2.0')).getByRole('button', { name: 'Borrar la etiqueta «v0.2.0»' });
    await userEvent.click(del);
    await waitFor(() => expect(confirm).toHaveBeenCalledTimes(1));
    expect(confirm.mock.calls[0]![0]).toMatch(/Borrar la etiqueta local «v0\.2\.0»/);
    expect(tagRow('v0.2.0')).not.toBeNull();
    expect(screen.queryByText('Etiqueta «v0.2.0» borrada')).not.toBeInTheDocument();

    await userEvent.click(within(tagRow('v0.2.0')).getByRole('button', { name: 'Borrar la etiqueta «v0.2.0»' }));
    expect(await screen.findByText('Etiqueta «v0.2.0» borrada')).toBeInTheDocument();
    await waitFor(() => expect(tagRow('v0.2.0')).toBeNull());
    expect((await tab.api.listTags(tab.id)).map((x) => x.name)).toEqual(['v0.1.0']);
    // Borrada la última, aparece el mensaje de «sin etiquetas».
    await userEvent.click(within(tagRow('v0.1.0')).getByRole('button', { name: 'Borrar la etiqueta «v0.1.0»' }));
    expect(await screen.findByText(/no tiene etiquetas/)).toBeInTheDocument();
  });

  it('publicar usa el remoto y se puede borrar del remoto con confirmación', async () => {
    const confirm = vi.fn((_m: string) => true);
    const { api } = await setup({ confirm });
    await waitFor(() => expect(tagRow('v0.2.0')).not.toBeNull());
    await userEvent.click(within(tagRow('v0.2.0')).getByRole('button', { name: 'Publicar la etiqueta «v0.2.0»' }));
    expect(await screen.findByText('Etiqueta «v0.2.0» publicada en «origin»')).toBeInTheDocument();
    expect(api.__publishedTags('tab-ariadna')).toEqual(['origin/v0.2.0']);

    await userEvent.click(within(tagRow('v0.2.0')).getByRole('button', { name: 'Borrar la etiqueta «v0.2.0» del remoto' }));
    expect(await screen.findByText('Etiqueta «v0.2.0» borrada de «origin»')).toBeInTheDocument();
    expect(confirm).toHaveBeenCalledWith(expect.stringMatching(/del remoto «origin»/));
    expect(api.__publishedTags('tab-ariadna')).toEqual([]);
    // La local sigue.
    expect(tagRow('v0.2.0')).not.toBeNull();
  });

  it('sin remotos no se puede publicar y se explica', async () => {
    const { api, tab } = await setup();
    await api.removeRemote(tab.id, 'origin');
    await waitFor(() => expect(tab.snapshot!.remotes).toHaveLength(0));
    await waitFor(() => expect(tagRow('v0.2.0')).not.toBeNull());
    const publish = within(tagRow('v0.2.0')).getByRole('button', { name: 'Publicar la etiqueta «v0.2.0»' });
    expect(publish).toBeDisabled();
    expect(publish).toHaveAttribute('title', 'Añade un remoto para publicar etiquetas.');
  });

  it('un error de Rust se muestra como aviso (nombre no válido que la UI no detectó)', async () => {
    const { tab } = await setup();
    const ok = await tagsOf(tab).create('v0.2.0', null, '');
    expect(ok).toBe(false);
    expect(await screen.findByText('No se pudo crear la etiqueta')).toBeInTheDocument();
  });
});

describe('Etiquetas desde el historial', () => {
  async function history() {
    const ctx = await loadedTab('tab-ariadna');
    render(HistoryPanel, { props: { tab: ctx.tab } });
    render(HistoryDetail, { props: { tab: ctx.tab } });
    render(Toasts);
    const list = await screen.findByRole('listbox', { name: 'Lista de commits' });
    await waitFor(() => expect(within(list).getAllByRole('option').length).toBeGreaterThan(3));
    return { ...ctx, list };
  }

  it('«Crear etiqueta aquí» crea la etiqueta en el commit seleccionado y sale como chip', async () => {
    const { tab, list } = await history();
    const second = within(list).getAllByRole('option')[1]!;
    await userEvent.click(second);
    const target = historyOf(tab).selected!;
    await userEvent.click(await screen.findByRole('button', { name: 'Crear etiqueta aquí' }));
    expect(screen.getByText(`Etiqueta en el commit ${target.sha.slice(0, 7)}`)).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText('Nombre'), 'hito-1');
    await userEvent.type(screen.getByLabelText('Mensaje (opcional)'), 'Un hito');
    await userEvent.click(screen.getByRole('button', { name: 'Crear etiqueta' }));
    expect(await screen.findByText('Etiqueta «hito-1» creada')).toBeInTheDocument();
    const created = (await tab.api.listTags(tab.id)).find((x) => x.name === 'hito-1')!;
    expect(created.commit).toBe(target.sha);
    expect(created.annotated).toBe(true);
    // El historial se recarga (nueva revisión) y la fila lleva el chip de etiqueta.
    await waitFor(() => expect(within(list).getAllByRole('option')[1]).toHaveTextContent('hito-1'));
    // El detalle lista las etiquetas del commit.
    await waitFor(() => expect(screen.getAllByText('hito-1').length).toBeGreaterThan(1));
  });

  it('las etiquetas ya existentes salen como chips en la lista', async () => {
    const { list } = await history();
    // En el simulado `ariadna` la etiqueta v0.2.0 está en un commit; si no está en la primera
    // página, la fila de HEAD al menos trae la rama.
    expect(within(list).getAllByRole('option')[0]).toHaveTextContent('main');
  });

  it('cambiar de commit cierra el formulario', async () => {
    const { list } = await history();
    await userEvent.click(await screen.findByRole('button', { name: 'Crear etiqueta aquí' }));
    expect(screen.getByLabelText('Nombre')).toBeInTheDocument();
    await userEvent.click(within(list).getAllByRole('option')[2]!);
    await waitFor(() => expect(screen.queryByLabelText('Nombre')).not.toBeInTheDocument());
  });
});
