import { cleanup, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { resetGlobals } from '../../test/helpers';
import { AZ, prTab } from '../../test/prHelpers';
import PrDetail from './PrDetail.svelte';
import Toasts from './Toasts.svelte';

beforeEach(resetGlobals);
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  resetGlobals();
});

async function open(id: number, opts: Parameters<typeof prTab>[0] = {}) {
  const ctx = await prTab(opts);
  await ctx.pr.loadList();
  render(PrDetail, { props: { tab: ctx.tab } });
  render(Toasts);
  ctx.pr.select(id);
  await screen.findByRole('heading', { level: 2, name: new RegExp(`#${id}`) });
  return ctx;
}

const region = () => screen.getByRole('region', { name: 'Detalle de la pull request' });

describe('Detalle: resumen (PR-03)', () => {
  it('sin selección invita a elegir o crear', async () => {
    const ctx = await prTab();
    render(PrDetail, { props: { tab: ctx.tab } });
    expect(screen.getByRole('heading', { name: 'Elige una pull request' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Nueva pull request' }));
    expect(ctx.pr.creating).toBe(true);
    expect(await screen.findByRole('form', { name: 'Nueva pull request' })).toBeInTheDocument();
  });

  it('con la lista cargada y vacía no repite el estado vacío ni el botón de crear', async () => {
    const ctx = await prTab({ scenario: 'pr-empty' });
    await ctx.pr.loadList();
    expect(ctx.pr.items).toHaveLength(0);
    const { container } = render(PrDetail, { props: { tab: ctx.tab } });
    expect(screen.queryByRole('heading', { name: 'Elige una pull request' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Nueva pull request' })).toBeNull();
    expect(container.querySelector('.pick-blank')).not.toBeNull();
  });

  it('muestra título, ramas, estado, revisores, políticas y commits; la descripción es texto', async () => {
    await open(101);
    expect(screen.getByRole('heading', { level: 2 })).toHaveTextContent('#101 Añade validación de formularios');
    expect(screen.getByLabelText('De la rama feature/pr-101 a la rama main')).toBeInTheDocument();
    expect(within(region()).getAllByText('Abierta').length).toBeGreaterThan(0);

    const desc = screen.getByRole('heading', { name: 'Descripción' }).closest('section') as HTMLElement;
    expect(within(desc).getByRole('heading', { level: 3, name: 'Resumen' })).toBeInTheDocument();
    expect(within(desc).getByText('Cambios de la PR 101.')).toBeInTheDocument();
    expect(desc.querySelector('script')).toBeNull();
    expect(desc).toHaveTextContent("<script>alert('esto es texto, no HTML')</script>");

    const reviewers = screen.getByRole('heading', { name: 'Revisores y votos' }).closest('section') as HTMLElement;
    expect(within(reviewers).getByText('Beto')).toBeInTheDocument();
    expect(within(reviewers).getByText('Aprobar')).toBeInTheDocument();
    expect(within(reviewers).getByText('Obligatorio')).toBeInTheDocument();

    const policies = screen.getByRole('heading', { name: 'Políticas y comprobaciones' }).closest('section') as HTMLElement;
    expect(within(policies).getAllByRole('listitem')).toHaveLength(3);
    expect(within(policies).getByText('Pendiente')).toBeInTheDocument();

    const commits = screen.getByRole('heading', { name: /Commits/ }).closest('section') as HTMLElement;
    expect(within(commits).getByText('feat: primera parte')).toBeInTheDocument();
  });

  it('Azure DevOps: políticas obligatorias y elementos de trabajo', async () => {
    await open(101, { url: AZ });
    const policies = screen.getByRole('heading', { name: 'Políticas y comprobaciones' }).closest('section') as HTMLElement;
    expect(within(policies).getAllByText('Obligatoria')).toHaveLength(2);
    expect(screen.getByRole('heading', { name: 'Elementos de trabajo' })).toBeInTheDocument();
    expect(screen.getByText('#1301')).toBeInTheDocument();
  });

  it('una PR con conflictos y checks fallidos lo señala', async () => {
    await open(105);
    expect(within(region()).getAllByText('Conflictos').length).toBeGreaterThan(0);
    expect(screen.getByText('Fallida')).toBeInTheDocument();
  });

  it('mientras carga muestra un esqueleto ocupado', async () => {
    const ctx = await prTab();
    vi.spyOn(ctx.api, 'getPullRequest').mockReturnValue(new Promise(() => {}));
    render(PrDetail, { props: { tab: ctx.tab } });
    ctx.pr.select(101);
    expect(await screen.findByRole('status', { name: 'Cargando la pull request' })).toHaveAttribute('aria-busy', 'true');
  });

  it('NOT_FOUND: avisa y «Volver a la lista» cierra el detalle', async () => {
    const ctx = await prTab();
    render(PrDetail, { props: { tab: ctx.tab } });
    ctx.pr.select(999);
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('No se encuentra');
    await userEvent.click(within(alert).getByRole('button', { name: 'Volver a la lista' }));
    expect(ctx.pr.selectedId).toBeNull();
    expect(screen.getByRole('heading', { name: 'Elige una pull request' })).toBeInTheDocument();
  });

  it('las pestañas se mueven con las flechas', async () => {
    await open(101);
    const user = userEvent.setup();
    const overview = screen.getByRole('tab', { name: 'Resumen' });
    overview.focus();
    expect(overview).toHaveAttribute('aria-selected', 'true');
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: /Archivos/ })).toHaveFocus();
    expect(screen.getByRole('tab', { name: /Archivos/ })).toHaveAttribute('aria-selected', 'true');
    await user.keyboard('{End}');
    expect(screen.getByRole('tab', { name: /Conversación/ })).toHaveFocus();
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: 'Resumen' })).toHaveFocus();
  });
});

describe('Revisar y votar (PR-07)', () => {
  it('GitHub ofrece aprobar, solicitar cambios y comentar; los dos últimos piden texto', async () => {
    const { pr } = await open(101);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Revisar' }));
    const form = screen.getByRole('form', { name: 'Tu revisión' });
    expect(within(form).getAllByRole('radio').map((r) => r.closest('label')?.textContent?.trim().split('\n')[0])).toEqual([
      expect.stringContaining('Aprobar'),
      expect.stringContaining('Solicitar cambios'),
      expect.stringContaining('Comentar')
    ]);
    const send = within(form).getByRole('button', { name: 'Enviar revisión' });
    expect(send).toBeEnabled();
    await user.click(within(form).getByRole('radio', { name: /Solicitar cambios/ }));
    expect(send).toBeDisabled();
    expect(within(form).getByLabelText('Comentario (obligatorio)')).toHaveAttribute('aria-required', 'true');
    await user.type(within(form).getByLabelText('Comentario (obligatorio)'), 'Falta cubrir el caso nulo');
    expect(send).toBeEnabled();
    await user.click(send);
    await waitFor(() => expect(pr.detail?.reviewers.some((r) => r.vote === 'reject')).toBe(true));
    expect(await screen.findByText('Revisión enviada')).toBeInTheDocument();
    expect(screen.queryByRole('form', { name: 'Tu revisión' })).toBeNull();
  });

  it('Azure DevOps ofrece sus cinco votos', async () => {
    await open(101, { url: AZ });
    await userEvent.click(screen.getByRole('button', { name: 'Revisar' }));
    const form = screen.getByRole('form', { name: 'Tu revisión' });
    expect(within(form).getAllByRole('radio')).toHaveLength(5);
    expect(within(form).getByRole('radio', { name: /Esperar al autor/ })).toBeInTheDocument();
    expect(within(form).getByRole('radio', { name: /Rechazar/ })).toBeInTheDocument();
    // En Azure el comentario es opcional.
    await userEvent.click(within(form).getByRole('radio', { name: /Aprobar con sugerencias/ }));
    expect(within(form).getByRole('button', { name: 'Enviar revisión' })).toBeEnabled();
  });

  it('si se cancela la confirmación nativa del rechazo no cambia nada y el formulario sigue', async () => {
    const asked: string[] = [];
    const { pr } = await open(101, { url: AZ, confirm: (m) => (asked.push(m), false) });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Revisar' }));
    await user.click(screen.getByRole('radio', { name: /Rechazar/ }));
    await user.click(screen.getByRole('button', { name: 'Enviar revisión' }));
    await waitFor(() => expect(asked).toHaveLength(1));
    expect(pr.detail?.reviewers.some((r) => r.vote === 'reject')).toBe(false);
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByRole('form', { name: 'Tu revisión' })).toBeInTheDocument();
  });

  it('una PR completada no ofrece votar, completar ni abandonar', async () => {
    const ctx = await prTab();
    ctx.pr.filter = 'closed';
    render(PrDetail, { props: { tab: ctx.tab } });
    ctx.pr.select(103);
    await screen.findByRole('heading', { level: 2, name: /#103/ });
    expect(screen.queryByRole('button', { name: 'Revisar' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Completar' })).toBeNull();
    expect(screen.getByText('Esta pull request ya está completada.')).toBeInTheDocument();
  });
});

describe('Completar, abandonar y reabrir (PR-08)', () => {
  it('GitHub: merge, squash y rebase; completa con la estrategia elegida', async () => {
    const asked: string[] = [];
    const { pr } = await open(101, { confirm: (m) => (asked.push(m), true) });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Completar' }));
    const form = screen.getByRole('form', { name: 'Completar pull request' });
    expect(within(form).getAllByRole('radio')).toHaveLength(3);
    expect(within(form).queryByLabelText(/auto-complete/)).toBeNull();
    await user.click(within(form).getByRole('radio', { name: /Squash/ }));
    await user.click(within(form).getByRole('button', { name: 'Completar pull request' }));
    await waitFor(() => expect(pr.detail?.state).toBe('completed'));
    expect(asked[0]).toContain('squash');
    expect(await screen.findByText('Pull request completada')).toBeInTheDocument();
    expect(screen.getByText('Esta pull request ya está completada.')).toBeInTheDocument();
  });

  it('Azure DevOps: cuatro estrategias, auto-complete y elementos de trabajo', async () => {
    await open(101, { url: AZ });
    await userEvent.click(screen.getByRole('button', { name: 'Completar' }));
    const form = screen.getByRole('form', { name: 'Completar pull request' });
    expect(within(form).getAllByRole('radio')).toHaveLength(4);
    expect(within(form).getByRole('checkbox', { name: /auto-complete/ })).toBeInTheDocument();
    expect(within(form).getByRole('checkbox', { name: /elementos de trabajo/ })).toBeInTheDocument();
    await userEvent.click(within(form).getByRole('checkbox', { name: /auto-complete/ }));
    expect(within(form).getByRole('button', { name: 'Activar auto-complete' })).toBeInTheDocument();
  });

  it('con conflictos avisa antes y, si el servidor lo rechaza, enseña el motivo (POLICY)', async () => {
    const { pr } = await open(105);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Completar' }));
    const form = screen.getByRole('form', { name: 'Completar pull request' });
    expect(within(form).getByText('Puede que no se pueda completar')).toBeInTheDocument();
    expect(within(form).getByText('Tiene conflictos de fusión.')).toBeInTheDocument();
    await user.click(within(form).getByRole('button', { name: 'Completar pull request' }));
    const err = (await screen.findByText('El servidor no lo permite')).closest('.prerr') as HTMLElement;
    expect(err).toHaveTextContent('conflictos de fusión');
    expect(pr.detail?.state).toBe('active');
    await user.click(within(err).getByRole('button', { name: 'Cerrar aviso' }));
    expect(screen.queryByText('El servidor no lo permite')).toBeNull();
  });

  it('GitHub: «Cerrar pull request» y luego «Reabrir»', async () => {
    const { pr } = await open(102);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Cerrar pull request' }));
    await waitFor(() => expect(pr.detail?.state).toBe('abandoned'));
    expect(screen.queryByRole('button', { name: 'Revisar' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Reabrir' }));
    await waitFor(() => expect(pr.detail?.state).toBe('active'));
    expect(screen.getByRole('button', { name: 'Completar' })).toBeInTheDocument();
  });

  it('Azure DevOps lo llama «Abandonar»', async () => {
    await open(102, { url: AZ });
    expect(screen.getByRole('button', { name: 'Abandonar' })).toBeInTheDocument();
  });

  it('cancelar la confirmación nativa deja la PR como estaba', async () => {
    const { pr } = await open(102, { confirm: () => false });
    await userEvent.click(screen.getByRole('button', { name: 'Cerrar pull request' }));
    await waitFor(() => expect(pr.busy).toBeNull());
    expect(pr.detail?.state).toBe('active');
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
