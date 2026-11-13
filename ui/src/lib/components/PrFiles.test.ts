import { cleanup, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { readFileSync } from 'node:fs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { resetGlobals } from '../../test/helpers';
import { prTab } from '../../test/prHelpers';
import PrDetail from './PrDetail.svelte';
import Toasts from './Toasts.svelte';

beforeEach(resetGlobals);
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  resetGlobals();
});

async function openFiles(id = 101, opts: Parameters<typeof prTab>[0] = {}) {
  const ctx = await prTab(opts);
  render(PrDetail, { props: { tab: ctx.tab } });
  render(Toasts);
  ctx.pr.select(id);
  await screen.findByRole('heading', { level: 2, name: new RegExp(`#${id}`) });
  await waitFor(() => expect(ctx.pr.threads.length).toBeGreaterThan(0));
  await userEvent.click(screen.getByRole('tab', { name: /Archivos/ }));
  await screen.findByRole('list', { name: 'Archivos cambiados' });
  return ctx;
}

const threadsRegion = () => screen.getByRole('region', { name: 'Comentarios del archivo' });

describe('Archivos: diff con comentarios en línea (PR-06)', () => {
  it('lista los archivos con estado y cambios, y enseña el diff del primero con el visor existente', async () => {
    await openFiles();
    const list = screen.getByRole('list', { name: 'Archivos cambiados' });
    const items = within(list).getAllByRole('button');
    expect(items).toHaveLength(3);
    expect(items[0]).toHaveTextContent('src/lib/api/api.ts');
    expect(items[0]).toHaveTextContent('+12');
    expect(items[0]).toHaveAttribute('aria-current', 'true');
    expect(within(items[0] as HTMLElement).getByText('Modificado')).toBeInTheDocument();
    expect(within(items[1] as HTMLElement).getByText('Añadido')).toBeInTheDocument();
    // Diff de solo lectura (CodeDiff): líneas añadidas y quitadas.
    const code = await screen.findByRole('region', { name: /src\/lib\/api\/api.ts/ });
    expect(within(code).getByText('return 2;', { exact: false })).toBeInTheDocument();
    expect(within(code).getByText('return 1;', { exact: false })).toBeInTheDocument();
  });

  it('el hilo anclado a la línea 12 sale en el panel del archivo y su marca en el diff', async () => {
    await openFiles();
    const th = threadsRegion();
    expect(within(th).getByText(/Y si/, { exact: false })).toBeInTheDocument();
    expect(within(th).getByText('Buen punto, lo cubro.')).toBeInTheDocument();
    expect(within(th).getByText('Activo')).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Línea 12: 1 comentario, ver hilo' })).toBeInTheDocument();
    // El número de hilos activos también está en la lista de archivos.
    expect(within(screen.getByRole('list', { name: 'Archivos cambiados' })).getByText('1 hilo activo')).toBeInTheDocument();
  });

  it('pulsar la marca de una línea con hilo lo resalta; en una línea libre abre el formulario', async () => {
    await openFiles();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Línea 12: 1 comentario, ver hilo' }));
    expect(threadsRegion().querySelector('.thread.hl')).not.toBeNull();
    expect(screen.queryByRole('form', { name: 'Nuevo comentario en una línea' })).toBeNull();

    // La línea 11 existe dos veces: la eliminada (lado anterior) y la añadida (lado nuevo).
    await user.click((await screen.findAllByRole('button', { name: 'Comentar la línea 11' }))[1] as HTMLElement);
    const form = await screen.findByRole('form', { name: 'Nuevo comentario en una línea' });
    expect(within(form).getByLabelText('Línea')).toHaveValue(11);
    expect(within(form).getByLabelText('Lado')).toHaveValue('new');
    await user.type(within(form).getByLabelText('Comentario'), 'Mejor devolver 3');
    await user.click(within(form).getByRole('button', { name: 'Comentar' }));
    await waitFor(() => expect(screen.queryByRole('form', { name: 'Nuevo comentario en una línea' })).toBeNull());
    expect(within(threadsRegion()).getByText('Mejor devolver 3')).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Línea 11: 1 comentario, ver hilo' })).toBeInTheDocument();
  });

  it('una línea eliminada se comenta en el lado «anterior»', async () => {
    const { pr } = await openFiles();
    const user = userEvent.setup();
    await user.click((await screen.findAllByRole('button', { name: 'Comentar la línea 11' }))[0] as HTMLElement);
    const form = await screen.findByRole('form', { name: 'Nuevo comentario en una línea' });
    expect(within(form).getByLabelText('Lado')).toHaveValue('old');
    await user.type(within(form).getByLabelText('Comentario'), 'Esto se pierde');
    await user.click(within(form).getByRole('button', { name: 'Comentar' }));
    await waitFor(() => expect(pr.threads.some((t) => t.anchor?.side === 'old')).toBe(true));
  });

  it('alternativa de teclado: «Comentar una línea…» con número de línea escrito', async () => {
    const { pr } = await openFiles();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Comentar una línea…' }));
    const form = screen.getByRole('form', { name: 'Nuevo comentario en una línea' });
    expect(within(form).getByLabelText('Comentario')).toHaveFocus();
    const line = within(form).getByLabelText('Línea');
    await user.clear(line);
    await user.type(line, '0');
    expect(within(form).getByRole('button', { name: 'Comentar' })).toBeDisabled();
    await user.clear(line);
    await user.type(line, '3');
    await user.type(within(form).getByLabelText('Comentario'), 'Revisa el nombre');
    await user.keyboard('{Control>}{Enter}{/Control}');
    await waitFor(() => expect(pr.threads.some((t) => t.anchor?.line === 3)).toBe(true));
  });

  it('cambiar de archivo cambia diff e hilos; los archivos nuevos no tienen comentarios', async () => {
    await openFiles();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /src\/lib\/util\/nuevo.ts/ }));
    expect(await screen.findByRole('region', { name: /nuevo.ts/ })).toBeInTheDocument();
    expect(within(threadsRegion()).queryByText('Buen punto, lo cubro.')).toBeNull();
    expect(within(threadsRegion()).getByText(/no tiene comentarios/)).toBeInTheDocument();
  });

  it('la cabecera de archivos agrupa lista y barra sobre el diff', async () => {
    await openFiles();
    const files = document.querySelector('.files') as HTMLElement;
    const head = files.querySelector('.fhead') as HTMLElement;
    expect(head).not.toBeNull();
    expect(files.firstElementChild).toBe(head);
    expect(head).toContainElement(screen.getByRole('list', { name: 'Archivos cambiados' }));
    expect(head).toContainElement(screen.getByRole('heading', { name: 'src/lib/api/api.ts' }));
    expect(head.nextElementSibling?.classList.contains('diffbox')).toBe(true);
  });

  it('la cabecera es fija y el diff tiene altura mínima (CSS del componente)', () => {
    const src = readFileSync('src/lib/components/PrFiles.svelte', 'utf8');
    expect(src).toMatch(/\.fhead\s*\{[^}]*position:\s*sticky/);
    expect(src).toMatch(/\.diffbox\s*\{[^}]*min-height:\s*clamp\(280px/);
    expect(src).toMatch(/\.files\s*\{[^}]*overflow-y:\s*auto/);
  });

  it('las flechas recorren la lista de archivos', async () => {
    await openFiles();
    const user = userEvent.setup();
    const items = within(screen.getByRole('list', { name: 'Archivos cambiados' })).getAllByRole('button');
    items[0]?.focus();
    await user.keyboard('{ArrowDown}');
    expect(items[1]).toHaveFocus();
    await user.keyboard('{ArrowDown}{ArrowDown}');
    expect(items[2]).toHaveFocus();
  });

  it('los archivos sin diff se explican con el motivo del servidor', async () => {
    const ctx = await prTab();
    const real = ctx.api.pullRequestDiff.bind(ctx.api);
    vi.spyOn(ctx.api, 'pullRequestDiff').mockImplementation(async (t, id) => {
      const d = await real(t, id);
      return { ...d, diffs: d.diffs.slice(1), omitted: [{ path: 'src/lib/api/api.ts', reason: 'Archivo demasiado grande para el servidor' }] };
    });
    render(PrDetail, { props: { tab: ctx.tab } });
    ctx.pr.select(101);
    await screen.findByRole('heading', { level: 2, name: /#101/ });
    await userEvent.click(screen.getByRole('tab', { name: /Archivos/ }));
    expect(await screen.findByText('Archivo demasiado grande para el servidor')).toBeInTheDocument();
    expect(screen.getByText('1 archivo sin diff (binario, enorme o solo renombrado)')).toBeInTheDocument();
  });

  it('un error al cargar el diff se muestra con reintento', async () => {
    const ctx = await prTab();
    vi.spyOn(ctx.api, 'pullRequestDiff').mockRejectedValueOnce(Object.assign(new Error('x'), { code: 'NETWORK', name: 'ApiError', detail: null }));
    render(PrDetail, { props: { tab: ctx.tab } });
    ctx.pr.select(101);
    await screen.findByRole('heading', { level: 2, name: /#101/ });
    await userEvent.click(screen.getByRole('tab', { name: /Archivos/ }));
    const alert = await screen.findByRole('alert');
    await userEvent.click(within(alert).getByRole('button', { name: 'Reintentar' }));
    expect(await screen.findByRole('list', { name: 'Archivos cambiados' })).toBeInTheDocument();
  });
});

describe('Conversación: hilos (PR-06)', () => {
  async function conversation(id = 101, opts: Parameters<typeof prTab>[0] = {}) {
    const ctx = await prTab(opts);
    render(PrDetail, { props: { tab: ctx.tab } });
    render(Toasts);
    ctx.pr.select(id);
    await screen.findByRole('heading', { level: 2, name: new RegExp(`#${id}`) });
    await waitFor(() => expect(ctx.pr.threads.length).toBeGreaterThan(0));
    await userEvent.click(screen.getByRole('tab', { name: /Conversación/ }));
    return ctx;
  }

  it('separa los comentarios generales de los de archivos', async () => {
    await conversation();
    expect(screen.getByText('Comentario general sobre la PR.')).toBeInTheDocument();
    const inline = screen.getByRole('heading', { name: /Comentarios en archivos/ });
    expect(inline).toBeInTheDocument();
    expect(screen.getByText('Buen punto, lo cubro.')).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /Conversación/ })).toHaveTextContent('2');
  });

  it('GitHub: un comentario general no se responde ni se resuelve (cada uno es su hilo)', async () => {
    await conversation();
    const general = screen.getByText('Comentario general sobre la PR.').closest('article') as HTMLElement;
    expect(within(general).queryByRole('button', { name: 'Responder' })).toBeNull();
    expect(within(general).queryByRole('button', { name: 'Resolver' })).toBeNull();
  });

  it('Azure DevOps: el general sí se resuelve', async () => {
    await conversation(101, { url: 'https://dev.azure.com/org/proj/_git/app' });
    const general = screen.getByText('Comentario general sobre la PR.').closest('article') as HTMLElement;
    expect(within(general).getByRole('button', { name: 'Resolver' })).toBeInTheDocument();
  });

  it('responde a un hilo con Ctrl+Intro', async () => {
    const { pr } = await conversation();
    const user = userEvent.setup();
    const thread = screen.getByText('Buen punto, lo cubro.').closest('article') as HTMLElement;
    await user.click(within(thread).getByRole('button', { name: 'Responder' }));
    const box = within(thread).getByLabelText('Tu respuesta');
    expect(box).toHaveFocus();
    await user.type(box, 'Gracias, **hecho**');
    await user.keyboard('{Control>}{Enter}{/Control}');
    await waitFor(() => expect(pr.threads.find((t) => t.id === '10101')?.comments).toHaveLength(3));
    expect(within(thread).getByText('hecho', { selector: 'strong' })).toBeInTheDocument();
    expect(within(thread).queryByLabelText('Tu respuesta')).toBeNull();
  });

  it('Escape cancela la respuesta', async () => {
    await conversation();
    const user = userEvent.setup();
    const thread = screen.getByText('Buen punto, lo cubro.').closest('article') as HTMLElement;
    await user.click(within(thread).getByRole('button', { name: 'Responder' }));
    await user.keyboard('{Escape}');
    expect(within(thread).queryByLabelText('Tu respuesta')).toBeNull();
  });

  it('resuelve y reabre un hilo', async () => {
    const { pr } = await conversation();
    const user = userEvent.setup();
    const thread = screen.getByText('Buen punto, lo cubro.').closest('article') as HTMLElement;
    await user.click(within(thread).getByRole('button', { name: 'Resolver' }));
    await waitFor(() => expect(pr.threads.find((t) => t.id === '10101')?.status).toBe('resolved'));
    expect(within(thread).getByText('Resuelto')).toBeInTheDocument();
    await user.click(within(thread).getByRole('button', { name: 'Reabrir hilo' }));
    await waitFor(() => expect(pr.threads.find((t) => t.id === '10101')?.status).toBe('active'));
  });

  it('crea un comentario general nuevo', async () => {
    const { pr } = await conversation();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Nuevo comentario' }));
    const form = screen.getByRole('form', { name: 'Nuevo comentario general' });
    const send = within(form).getByRole('button', { name: 'Comentar' });
    expect(send).toBeDisabled();
    await user.type(within(form).getByLabelText('Comentario'), 'Una duda general');
    await user.click(send);
    await waitFor(() => expect(pr.threads.some((t) => t.anchor === null && t.comments[0]?.body === 'Una duda general')).toBe(true));
    expect(screen.queryByRole('form', { name: 'Nuevo comentario general' })).toBeNull();
    expect(screen.getByText('Una duda general')).toBeInTheDocument();
  });

  it('«Ver en archivos» salta al archivo y resalta el hilo', async () => {
    const { pr } = await conversation();
    await userEvent.click(screen.getByRole('button', { name: 'Ver en archivos' }));
    expect(pr.view).toBe('files');
    await screen.findByRole('list', { name: 'Archivos cambiados' });
    await waitFor(() => expect(threadsRegion().querySelector('.thread.hl')).not.toBeNull());
  });

  it('los comentarios en Markdown no inyectan HTML', async () => {
    const ctx = await prTab();
    const real = ctx.api.pullRequestThreads.bind(ctx.api);
    vi.spyOn(ctx.api, 'pullRequestThreads').mockImplementation(async (t, id) => {
      const list = await real(t, id);
      const first = list[0];
      if (first) first.comments[0] = { id: 'x', author: first.comments[0]!.author, body: '<img src=x onerror=alert(1)> [clic](javascript:alert(1))', createdAt: null };
      return list;
    });
    const { container } = render(PrDetail, { props: { tab: ctx.tab } });
    ctx.pr.select(101);
    await screen.findByRole('heading', { level: 2, name: /#101/ });
    await waitFor(() => expect(ctx.pr.threads.length).toBeGreaterThan(0));
    await userEvent.click(screen.getByRole('tab', { name: /Conversación/ }));
    expect(container.querySelector('img')).toBeNull();
    expect(screen.queryByRole('link', { name: 'clic' })).toBeNull();
  });
});
