import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { readyApp, resetGlobals } from '../../test/helpers';
import Welcome from './Welcome.svelte';

beforeEach(resetGlobals);
afterEach(resetGlobals);

async function setup(opts = {}) {
  const ctx = await readyApp(opts);
  ctx.app.newTab();
  const id = ctx.app.activeId!;
  render(Welcome, { props: { app: ctx.app, entryId: id } });
  return { ...ctx, id };
}

describe('Bienvenida', () => {
  it('separa favoritos de recientes', async () => {
    await setup();
    const fav = screen.getByRole('heading', { name: /Favoritos/ }).parentElement!;
    expect(within(fav).getByText('ariadna')).toBeInTheDocument();
    expect(within(fav).getByText('legacy-api')).toBeInTheDocument();
    expect(within(fav).queryByText('dotfiles')).not.toBeInTheDocument();
    const rec = screen.getByRole('heading', { name: /Recientes/ }).parentElement!;
    expect(within(rec).getByText('dotfiles')).toBeInTheDocument();
    expect(within(rec).queryByText('ariadna')).not.toBeInTheDocument();
  });

  it('la estrella marca y desmarca favorito', async () => {
    const { app } = await setup();
    const star = screen.getByRole('button', { name: 'Añadir dotfiles a favoritos' });
    expect(star).toHaveAttribute('aria-pressed', 'false');
    await userEvent.click(star);
    await waitFor(() => expect(app.recents.find((r) => r.name === 'dotfiles')?.isFavorite).toBe(true));
    const fav = screen.getByRole('heading', { name: /Favoritos/ }).parentElement!;
    expect(await within(fav).findByText('dotfiles')).toBeInTheDocument();
    await userEvent.click(within(fav).getByRole('button', { name: 'Quitar dotfiles de favoritos' }));
    await waitFor(() => expect(app.recents.find((r) => r.name === 'dotfiles')?.isFavorite).toBe(false));
  });

  it('olvidar quita el reciente de la lista', async () => {
    const { app } = await setup();
    await userEvent.click(screen.getByRole('button', { name: /^Olvidar apuntes-rust/ }));
    await waitFor(() => expect(app.recents.some((r) => r.name === 'apuntes-rust')).toBe(false));
    expect(screen.queryByText('apuntes-rust')).not.toBeInTheDocument();
  });

  it('abrir un reciente sustituye la pestaña de bienvenida por el repo', async () => {
    const { app, id } = await setup();
    await userEvent.click(screen.getByRole('button', { name: 'Abrir dotfiles' }));
    await waitFor(() => expect(app.entries.some((e) => e.kind === 'repo' && e.info.name === 'dotfiles')).toBe(true));
    expect(app.entries.some((e) => e.id === id)).toBe(false);
    expect(app.activeEntry?.kind).toBe('repo');
  });

  it('abrir un repo ya abierto enfoca la pestaña existente (no duplica)', async () => {
    const { app } = await setup();
    const before = app.entries.length;
    await userEvent.click(screen.getByRole('button', { name: 'Abrir ariadna' }));
    await waitFor(() => expect(app.activeId).toBe('tab-ariadna'));
    expect(app.entries.length).toBe(before - 1); // se descartó el borrador
    expect(app.entries.filter((e) => e.kind === 'repo' && e.info.path.endsWith('/ariadna'))).toHaveLength(1);
  });

  it('ruta manual: abre, y si no es un repo muestra el error en línea', async () => {
    const { app } = await setup();
    const input = screen.getByRole('textbox', { name: 'Ruta del repositorio' });
    const open = screen.getByRole('button', { name: 'Abrir' });
    expect(open).toBeDisabled();
    await userEvent.type(input, '/tmp/no-repo');
    await userEvent.click(open);
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('no es un repositorio de Git');
    expect(input).toHaveAttribute('aria-invalid', 'true');
    await userEvent.clear(input);
    await userEvent.type(input, '/home/javie/proyectos/otro');
    await userEvent.click(open);
    await waitFor(() => expect(app.entries.some((e) => e.kind === 'repo' && e.info.name === 'otro')).toBe(true));
  });

  it('Examinar usa el selector nativo', async () => {
    const { app } = await setup();
    await userEvent.click(screen.getByRole('button', { name: 'Examinar…' }));
    await waitFor(() => expect(app.entries.some((e) => e.kind === 'repo' && e.info.name === 'demo-nuevo')).toBe(true));
  });

  it('estado vacío cuando no hay recientes', async () => {
    await setup({ scenario: 'empty' });
    expect(screen.getByText('Aún no has abierto ningún repositorio')).toBeInTheDocument();
  });

  it('sin git: aviso con instrucciones y acciones desactivadas', async () => {
    await setup({ scenario: 'nogit' });
    expect(screen.getByText('Git no está disponible')).toBeInTheDocument();
    expect(screen.getByText(/Instala Git 2.39/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Examinar…' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Abrir ariadna' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Volver a comprobar' })).toBeEnabled();
  });
});
