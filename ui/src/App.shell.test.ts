import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import App from './App.svelte';
import { createMockApi } from '$lib/api/mockApi';
import { AppStore, SOFT_TAB_LIMIT } from '$lib/stores/app.svelte';
import { toasts } from '$lib/stores/toasts.svelte';
import { resetGlobals } from './test/helpers';

beforeEach(resetGlobals);
afterEach(resetGlobals);

function boot(opts = {}) {
  const api = createMockApi({ latency: 0, confirm: () => true, ...opts });
  const app = new AppStore(api);
  render(App, { props: { app } });
  return { api, app };
}

describe('atajos globales y menú nativo', () => {
  it('Ctrl+O abre el diálogo de carpeta y el repositorio en una pestaña nueva', async () => {
    const { app } = boot();
    await screen.findByRole('tab', { name: /ariadna/ });
    const before = app.entries.length;
    await userEvent.keyboard('{Control>}o{/Control}');
    await waitFor(() => expect(app.entries.length).toBe(before + 1));
    expect(app.activeEntry).toMatchObject({ kind: 'repo', info: { path: '/home/javie/proyectos/demo-nuevo' } });
  });

  it('Ctrl+O desde una pestaña «nueva» reutiliza esa pestaña', async () => {
    const { app } = boot();
    await screen.findByRole('tab', { name: /ariadna/ });
    await userEvent.keyboard('{Control>}t{/Control}');
    await screen.findByRole('heading', { name: 'Bienvenida a Ariadna' });
    const count = app.entries.length;
    await userEvent.keyboard('{Control>}o{/Control}');
    await waitFor(() => expect(app.activeEntry?.kind).toBe('repo'));
    expect(app.entries.length).toBe(count);
  });

  it('Ctrl+R refresca la pestaña activa y siempre se consume (la WebView no recarga la página)', async () => {
    const { api } = boot();
    await screen.findByRole('grid', { name: /Archivos con cambios/ });
    let calls = 0;
    const original = api.refresh.bind(api);
    api.refresh = async (id) => {
      calls++;
      return original(id);
    };
    const ev = new KeyboardEvent('keydown', { key: 'r', ctrlKey: true, bubbles: true, cancelable: true });
    window.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(true);
    expect(calls).toBe(1);
  });

  it('Ctrl+R sin repositorio abierto tampoco deja que la WebView recargue', async () => {
    boot({ scenario: 'empty' });
    await screen.findByRole('heading', { name: 'Bienvenida a Ariadna' });
    const ev = new KeyboardEvent('keydown', { key: 'r', ctrlKey: true, bubbles: true, cancelable: true });
    window.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(true);
  });

  it('un elemento del menú nativo ejecuta el mismo atajo (Archivo → Nueva pestaña)', async () => {
    const { api, app } = boot();
    await screen.findByRole('tab', { name: /ariadna/ });
    api.__menuAction('tab.new');
    await waitFor(() => expect(app.entries.some((e) => e.kind === 'new')).toBe(true));
    api.__menuAction('tab.close');
    await waitFor(() => expect(app.entries.some((e) => e.kind === 'new')).toBe(false));
  });

  it('una acción de menú desconocida no hace nada', async () => {
    const { api, app } = boot();
    await screen.findByRole('tab', { name: /ariadna/ });
    const n = app.entries.length;
    api.__menuAction('no.existe');
    await new Promise((r) => setTimeout(r, 10));
    expect(app.entries.length).toBe(n);
  });
});

describe('tope de 20 pestañas', () => {
  it('abrir la pestaña 21 avisa con un mensaje claro y no impide abrirla', async () => {
    const { api, app } = boot();
    await screen.findByRole('tab', { name: /ariadna/ });
    for (let i = app.entries.length; i < SOFT_TAB_LIMIT; i++) {
      const out = await app.openPath(`/home/javie/proyectos/r${i}`);
      expect(out.ok).toBe(true);
    }
    expect(app.entries.length).toBe(SOFT_TAB_LIMIT);
    toasts.clear();
    const out = await app.openPath('/home/javie/proyectos/r-extra');
    expect(out.ok).toBe(true);
    expect(app.entries.length).toBe(SOFT_TAB_LIMIT + 1);
    const warn = toasts.items.find((x) => x.kind === 'warning');
    expect(warn?.title).toBe('Muchas pestañas abiertas');
    expect(warn?.message).toMatch(/21 pestañas abiertas. Más de 20/);
    void api;
  });

  it('una pestaña «nueva» en el límite avisa una sola vez, también al abrir el repo en ella', async () => {
    const { app } = boot();
    await screen.findByRole('tab', { name: /ariadna/ });
    for (let i = app.entries.length; i < SOFT_TAB_LIMIT; i++) await app.openPath(`/home/javie/proyectos/r${i}`);
    toasts.clear();
    app.newTab();
    const draft = app.activeId!;
    expect(toasts.items.filter((x) => x.kind === 'warning')).toHaveLength(1);
    await app.openPath('/home/javie/proyectos/r-extra', draft);
    expect(toasts.items.filter((x) => x.kind === 'warning')).toHaveLength(1);
  });
});
