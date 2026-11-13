import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from './App.svelte';
import { AppStore } from '$lib/stores/app.svelte';
import { mockApi, resetGlobals } from './test/helpers';

beforeEach(resetGlobals);
afterEach(resetGlobals);

function boot(opts = {}) {
  const app = new AppStore(mockApi(opts));
  render(App, { props: { app } });
  return app;
}

describe('App (integración con mockApi)', () => {
  it('arranca con la tira de pestañas y el workspace de la pestaña activa', async () => {
    boot();
    expect(await screen.findByRole('tab', { name: /^ariadna / })).toHaveAttribute('aria-selected', 'true');
    expect(await screen.findByRole('grid', { name: /Archivos con cambios/ })).toBeInTheDocument();
    expect(screen.getByRole('tabpanel', { name: /ariadna/ })).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Asunto' })).toBeInTheDocument();
  });

  it('muestra el esqueleto de arranque y después el contenido', async () => {
    boot({ latency: 20 });
    expect(screen.getByLabelText('Cargando Ariadna…')).toBeInTheDocument();
    await screen.findByRole('tab', { name: /ariadna/ });
  });

  it('error al leer el estado: estado de error con reintento', async () => {
    boot({ scenario: 'error' });
    expect(await screen.findByText('No se pudo iniciar Ariadna')).toBeInTheDocument();
    expect(screen.getByText('No se pudo leer el estado de la aplicación.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Reintentar' })).toBeInTheDocument();
  });

  it('Ctrl+T abre una pestaña de bienvenida y Ctrl+W la cierra', async () => {
    const app = boot();
    await screen.findByRole('tab', { name: /ariadna/ });
    await userEvent.keyboard('{Control>}t{/Control}');
    expect(await screen.findByRole('tab', { name: 'Nueva pestaña', selected: true })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Bienvenida a Ariadna' })).toBeInTheDocument();
    await userEvent.keyboard('{Control>}w{/Control}');
    await waitFor(() => expect(app.entries.some((e) => e.kind === 'new')).toBe(false));
  });

  it('Ctrl+2 va a la pestaña 2 y muestra el aviso de conflictos', async () => {
    const app = boot();
    await screen.findByRole('tab', { name: /ariadna/ });
    await userEvent.keyboard('{Control>}2{/Control}');
    await waitFor(() => expect(app.activeId).toBe('tab-legacy'));
    expect((await screen.findAllByText('Fusión en curso')).length).toBeGreaterThan(0);
    // Banner de la pestaña con el contador y acceso directo al estudio de conflictos.
    expect(await screen.findByText('4 archivos en conflicto por resolver')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Resolver conflictos' })).toBeInTheDocument();
  });

  it('pestaña con repo desaparecido: banner con Reintentar, Localizar y Cerrar', async () => {
    const app = boot();
    await screen.findByRole('tab', { name: /ariadna/ });
    await userEvent.keyboard('{Control>}3{/Control}');
    await waitFor(() => expect(app.activeId).toBe('tab-antiguo'));
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('No se encuentra el repositorio');
    expect(screen.getByRole('button', { name: 'Reintentar' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Localizar/ })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Cerrar pestaña' }));
    await waitFor(() => expect(app.entries.some((e) => e.id === 'tab-antiguo')).toBe(false));
  });

  it('Ctrl+K abre la paleta, filtra de forma difusa y ejecuta; Esc cierra y devuelve el foco', async () => {
    boot();
    await screen.findByRole('tab', { name: /ariadna/ });
    await userEvent.keyboard('{Control>}k{/Control}');
    const dialog = await screen.findByRole('dialog', { name: 'Paleta de comandos' });
    const input = screen.getByRole('combobox');
    expect(input).toHaveFocus();
    await userEvent.type(input, 'tmoscr');
    const opts = screen.queryAllByRole('option').map((o) => o.textContent);
    expect(opts.join('|')).toMatch(/Tema: oscuro/);
    await userEvent.keyboard('{Enter}');
    await waitFor(() => expect(document.documentElement.getAttribute('data-theme')).toBe('dark'));
    expect(dialog).not.toBeInTheDocument();

    await userEvent.keyboard('{Control>}k{/Control}');
    await screen.findByRole('dialog');
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('la paleta atrapa el foco con Tab', async () => {
    boot();
    await screen.findByRole('tab', { name: /ariadna/ });
    await userEvent.keyboard('{Control>}k{/Control}');
    const dialog = await screen.findByRole('dialog');
    for (let i = 0; i < 6; i++) {
      await userEvent.tab();
      expect(dialog.contains(document.activeElement)).toBe(true);
    }
  });

  it('el tema y la densidad se aplican al documento', async () => {
    boot();
    await screen.findByRole('tab', { name: /ariadna/ });
    await userEvent.click(screen.getByRole('button', { name: /^Tema:/ }));
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    await userEvent.click(screen.getByRole('button', { name: /^Tema:/ }));
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    await userEvent.click(screen.getByRole('button', { name: /^Tema:/ }));
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false); // sistema
    await userEvent.click(screen.getByRole('button', { name: 'Alternar densidad compacta' }));
    expect(document.documentElement.getAttribute('data-density')).toBe('compact');
  });

  it('la barra de título tiene región arrastrable de Tauri', async () => {
    boot();
    await screen.findByRole('tab', { name: /ariadna/ });
    expect(document.querySelector('header[data-tauri-drag-region]')).not.toBeNull();
  });
});

describe('vigilancia de archivos: foco de la ventana', () => {
  it('al recuperar el foco de la ventana refresca la pestaña activa', async () => {
    const api = mockApi();
    const spy = vi.spyOn(api, 'refresh');
    const app = new AppStore(api);
    render(App, { props: { app } });
    await screen.findByRole('grid', { name: /Archivos con cambios/ });
    spy.mockClear();
    window.dispatchEvent(new Event('focus'));
    expect(spy).toHaveBeenCalledWith('tab-ariadna');
  });
});
