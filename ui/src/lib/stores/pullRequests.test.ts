import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PullRequestPage } from '$lib/api/prTypes';
import { prOf } from './pullRequests.svelte';
import { toasts } from './toasts.svelte';
import { resetGlobals } from '../../test/helpers';
import { AZ, prTab } from '../../test/prHelpers';

beforeEach(resetGlobals);
afterEach(() => {
  resetGlobals();
  vi.restoreAllMocks();
});

const ids = (pr: { items: { id: number }[] }) => pr.items.map((p) => p.id);

describe('soporte (PR-01)', () => {
  it('GitHub y Azure DevOps admiten PR; otro servidor no', async () => {
    const gh = await prTab();
    expect(gh.pr.supported).toBe(true);
    expect(gh.pr.caps?.votes).toEqual(['approve', 'reject', 'comment']);
    expect(gh.pr.caps?.strategies).toEqual(['merge', 'squash', 'rebase']);

    const az = await prTab({ url: AZ });
    expect(az.pr.caps?.votes).toHaveLength(5);
    expect(az.pr.caps?.workItems).toBe(true);

    const other = await prTab({ url: 'https://gitlab.com/g/r.git' });
    expect(other.pr.supportLoaded).toBe(true);
    expect(other.pr.supported).toBe(false);
    expect(other.pr.support?.reason).toMatch(/navegador/);
  });

  it('un fallo al preguntar deja el error y no marca soporte', async () => {
    const { pr, api } = await prTab();
    vi.spyOn(api, 'pullRequestSupport').mockRejectedValueOnce(new Error('boom'));
    await pr.loadSupport();
    expect(pr.supportError).not.toBeNull();
    expect(pr.supported).toBe(false);
    expect(pr.supportLoaded).toBe(true);
  });
});

describe('lista, filtros y búsqueda (PR-02)', () => {
  it('abiertas, mías, donde soy revisor y cerradas', async () => {
    const { pr } = await prTab();
    await pr.loadList();
    expect(ids(pr)).toEqual([105, 102, 101]);
    await pr.setFilter('mine');
    expect(ids(pr)).toEqual([102]);
    await pr.setFilter('reviewing');
    expect(ids(pr)).toEqual([101]);
    await pr.setFilter('closed');
    expect(ids(pr)).toEqual([104, 103]);
    expect(pr.filter).toBe('closed');
  });

  it('la búsqueda filtra por título, número, autor o rama sin llamar al servidor', async () => {
    const { pr, api } = await prTab();
    await pr.loadList();
    const spy = vi.spyOn(api, 'listPullRequests');
    pr.query = 'factura';
    expect(pr.visible.map((p) => p.id)).toEqual([105]);
    pr.query = '#101';
    expect(pr.visible.map((p) => p.id)).toEqual([101]);
    pr.query = 'ana';
    expect(pr.visible.map((p) => p.id)).toEqual([105, 101]);
    pr.query = 'feature/pr-102';
    expect(pr.visible.map((p) => p.id)).toEqual([102]);
    pr.query = 'no-existe';
    expect(pr.visible).toEqual([]);
    expect(spy).not.toHaveBeenCalled();
  });

  it('pagina hacia delante y hacia atrás', async () => {
    const { pr, api } = await prTab();
    const page = (n: number): PullRequestPage => ({ items: [], page: n, hasMore: n < 3 });
    const spy = vi.spyOn(api, 'listPullRequests').mockImplementation(async (_t, _f, n) => page(n));
    await pr.loadList(1);
    expect(pr.hasMore).toBe(true);
    await pr.nextPage();
    await pr.nextPage();
    expect(pr.page).toBe(3);
    expect(pr.hasMore).toBe(false);
    await pr.nextPage(); // no hay más: no llama
    expect(spy).toHaveBeenCalledTimes(3);
    await pr.prevPage();
    expect(pr.page).toBe(2);
    await pr.setFilter('mine');
    expect(pr.page).toBe(1);
  });

  it('una respuesta lenta de un filtro anterior no pisa a la nueva', async () => {
    const { pr, api } = await prTab();
    const real = api.listPullRequests.bind(api);
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    vi.spyOn(api, 'listPullRequests').mockImplementation(async (t, f, p) => {
      const res = await real(t, f, p);
      if (f === 'open') await gate;
      return res;
    });
    const slow = pr.loadList(1);
    await pr.setFilter('closed');
    release();
    await slow;
    expect(pr.filter).toBe('closed');
    expect(ids(pr)).toEqual([104, 103]);
  });

  it('escenarios: sin permiso, límite, sin red y vacío', async () => {
    const auth = await prTab({ scenario: 'pr-auth' });
    await auth.pr.loadList();
    expect(auth.pr.listError).toMatchObject({ code: 'AUTH', detail: 'pull_requests=read' });

    const rate = await prTab({ scenario: 'pr-rate-limit' });
    await rate.pr.loadList();
    expect(rate.pr.listError).toMatchObject({ code: 'RATE_LIMITED', detail: '30' });

    const net = await prTab({ scenario: 'pr-network' });
    await net.pr.loadList();
    expect(net.pr.listError?.code).toBe('NETWORK');

    const empty = await prTab({ scenario: 'pr-empty' });
    await empty.pr.loadList();
    expect(empty.pr.listError).toBeNull();
    expect(empty.pr.items).toEqual([]);
    expect(empty.pr.listLoaded).toBe(true);
  });
});

describe('detalle (PR-03)', () => {
  it('seleccionar carga el detalle y los hilos; el diff, solo al abrir Archivos', async () => {
    const { pr, api } = await prTab();
    await pr.loadList();
    const diff = vi.spyOn(api, 'pullRequestDiff');
    pr.select(101);
    await vi.waitFor(() => expect(pr.detail?.id).toBe(101));
    await vi.waitFor(() => expect(pr.threads).toHaveLength(2));
    expect(diff).not.toHaveBeenCalled();
    pr.setView('files');
    await vi.waitFor(() => expect(pr.diff?.files).toHaveLength(3));
    expect(diff).toHaveBeenCalledTimes(1);
    pr.setView('overview');
    pr.setView('files');
    expect(diff).toHaveBeenCalledTimes(1);
  });

  it('un detalle lento de otra PR no sustituye al de la seleccionada', async () => {
    const { pr, api } = await prTab();
    const real = api.getPullRequest.bind(api);
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    vi.spyOn(api, 'getPullRequest').mockImplementation(async (t, id) => {
      const d = await real(t, id);
      if (id === 101) await gate;
      return d;
    });
    pr.select(101);
    pr.select(102);
    await vi.waitFor(() => expect(pr.detail?.id).toBe(102));
    release();
    await new Promise((r) => setTimeout(r, 5));
    expect(pr.detail?.id).toBe(102);
  });

  it('NOT_FOUND queda en detailError', async () => {
    const { pr } = await prTab();
    pr.select(999);
    await vi.waitFor(() => expect(pr.detailError?.code).toBe('NOT_FOUND'));
    expect(pr.detail).toBeNull();
  });
});

describe('votar (PR-07)', () => {
  it('aprobar actualiza el detalle y la fila de la lista', async () => {
    const { pr } = await prTab();
    await pr.loadList();
    pr.select(101);
    await vi.waitFor(() => expect(pr.detail?.id).toBe(101));
    expect(await pr.vote('approve', null)).toBe(true);
    expect(pr.detail?.reviewers.find((r) => r.user.login === 'yo@ejemplo.test')?.vote).toBe('approve');
    expect(pr.items.find((p) => p.id === 101)?.reviewers.find((r) => r.user.login === 'yo@ejemplo.test')?.vote).toBe('approve');
    expect(toasts.items.some((x) => x.title === 'Revisión enviada')).toBe(true);
  });

  it('el rechazo abre la confirmación nativa de Rust; si se cancela no cambia nada', async () => {
    const asked: string[] = [];
    const { pr } = await prTab({ confirm: (m) => (asked.push(m), false) });
    await pr.loadList();
    pr.select(101);
    await vi.waitFor(() => expect(pr.detail?.id).toBe(101));
    const before = pr.detail;
    expect(await pr.vote('reject', 'No me convence')).toBe(false);
    expect(asked).toHaveLength(1);
    expect(asked[0]).toContain('feature/pr-101 → main');
    expect(pr.detail).toBe(before);
    expect(pr.actionError).toBeNull();
  });

  it('GitHub exige comentario al pedir cambios (INVALID_REFERENCE)', async () => {
    const { pr } = await prTab();
    pr.select(101);
    await vi.waitFor(() => expect(pr.detail?.id).toBe(101));
    expect(await pr.vote('reject', '  ')).toBe(false);
    expect(pr.actionError?.code).toBe('INVALID_REFERENCE');
  });
});

describe('completar, abandonar y reabrir (PR-08)', () => {
  it('completar con squash la pasa a completada y la saca de «abiertas»', async () => {
    const asked: string[] = [];
    const { pr } = await prTab({ confirm: (m) => (asked.push(m), true) });
    await pr.loadList();
    pr.select(101);
    await vi.waitFor(() => expect(pr.detail?.id).toBe(101));
    expect(await pr.complete({ strategy: 'squash', deleteSourceBranch: true, completeWorkItems: false, autoComplete: false, commitMessage: null })).toBe(true);
    expect(asked[0]).toContain('squash');
    expect(pr.detail?.state).toBe('completed');
    await vi.waitFor(() => expect(ids(pr)).not.toContain(101));
  });

  it('una política incumplida o un conflicto llega como POLICY con el motivo', async () => {
    const { pr } = await prTab();
    pr.select(105);
    await vi.waitFor(() => expect(pr.detail?.id).toBe(105));
    expect(await pr.complete({ strategy: 'merge', deleteSourceBranch: false, completeWorkItems: false, autoComplete: false, commitMessage: null })).toBe(false);
    expect(pr.actionError).toMatchObject({ code: 'POLICY' });
    expect(pr.actionError?.message).toMatch(/conflictos/);
    expect(pr.detail?.state).toBe('active');
    pr.dismissActionError();
    expect(pr.actionError).toBeNull();
  });

  it('abandonar (cerrar en GitHub) y reabrir', async () => {
    const { pr } = await prTab();
    pr.select(102);
    await vi.waitFor(() => expect(pr.detail?.id).toBe(102));
    expect(await pr.abandon()).toBe(true);
    expect(pr.detail?.state).toBe('abandoned');
    expect(toasts.items.some((x) => x.title === 'Pull request cerrada')).toBe(true);
    expect(await pr.reopen()).toBe(true);
    expect(pr.detail?.state).toBe('active');
  });

  it('abandonar con la confirmación nativa cancelada no hace nada', async () => {
    const { pr } = await prTab({ confirm: () => false });
    pr.select(102);
    await vi.waitFor(() => expect(pr.detail?.id).toBe(102));
    expect(await pr.abandon()).toBe(false);
    expect(pr.detail?.state).toBe('active');
  });

  it('una completada no se puede reabrir (POLICY)', async () => {
    const { pr } = await prTab();
    pr.filter = 'closed';
    pr.select(103);
    await vi.waitFor(() => expect(pr.detail?.id).toBe(103));
    expect(await pr.reopen()).toBe(false);
    expect(pr.actionError?.code).toBe('POLICY');
  });

  it('no lanza dos acciones a la vez', async () => {
    const { pr, api } = await prTab();
    pr.select(101);
    await vi.waitFor(() => expect(pr.detail?.id).toBe(101));
    const spy = vi.spyOn(api, 'votePullRequest');
    const [a, b] = await Promise.all([pr.vote('approve', null), pr.vote('approve', null)]);
    expect([a, b].filter(Boolean)).toHaveLength(1);
    expect(spy).toHaveBeenCalledTimes(1);
  });
});

describe('hilos (PR-06)', () => {
  it('comenta, responde y resuelve', async () => {
    const { pr } = await prTab();
    pr.select(101);
    await vi.waitFor(() => expect(pr.threads).toHaveLength(2));

    const general = await pr.comment({ kind: 'new', body: 'Un comentario general' });
    expect(general?.anchor).toBeNull();
    expect(pr.threads).toHaveLength(3);

    const inline = await pr.comment({ kind: 'new', body: 'Aquí', anchor: { path: 'src/lib/api/api.ts', line: 14, side: 'new' } });
    expect(inline?.anchor).toMatchObject({ line: 14 });
    expect(pr.threads).toHaveLength(4);

    const reply = await pr.comment({ kind: 'reply', threadId: '10101', body: 'Respuesta' });
    expect(reply?.comments).toHaveLength(3);
    expect(pr.threads.find((x) => x.id === '10101')?.comments).toHaveLength(3);

    expect(await pr.resolve('10101', true)).toBe(true);
    expect(pr.threads.find((x) => x.id === '10101')?.status).toBe('resolved');
    expect(pr.activeThreads).toBe(3);
    expect(await pr.resolve('10101', false)).toBe(true);
    expect(pr.threads.find((x) => x.id === '10101')?.status).toBe('active');
  });

  it('un comentario vacío lo rechaza el servidor y se ve como error de acción', async () => {
    const { pr } = await prTab();
    pr.select(101);
    await vi.waitFor(() => expect(pr.threads).toHaveLength(2));
    expect(await pr.comment({ kind: 'new', body: '  ' })).toBeNull();
    expect(pr.actionError?.code).toBe('INVALID_REFERENCE');
  });

  it('en GitHub no se responde a un comentario general', async () => {
    const { pr } = await prTab();
    pr.select(101);
    await vi.waitFor(() => expect(pr.threads).toHaveLength(2));
    expect(await pr.comment({ kind: 'reply', threadId: 'issue:10102', body: 'x' })).toBeNull();
    expect(pr.actionError?.code).toMatch(/INVALID_REFERENCE|NOT_FOUND/);
  });
});

describe('crear (PR-04)', () => {
  it('crea la PR, la selecciona y refresca la lista', async () => {
    const { pr } = await prTab();
    await pr.loadList();
    pr.startCreate();
    await vi.waitFor(() => expect(pr.templateLoaded).toBe(true));
    expect(pr.template).toContain('## Qué cambia');
    const d = await pr.create({ title: 'Mi cambio', description: 'texto', sourceBranch: 'rust-migration', targetBranch: 'main', draft: true, reviewers: ['beto'], workItemIds: [] });
    expect(d?.id).toBeGreaterThanOrEqual(200);
    expect(pr.creating).toBe(false);
    expect(pr.selectedId).toBe(d?.id);
    expect(pr.detail?.isDraft).toBe(true);
    await vi.waitFor(() => expect(ids(pr)).toContain(d?.id));
  });

  it('duplicada → CONFLICT, y se queda el formulario con el error', async () => {
    const { pr } = await prTab();
    pr.startCreate();
    const draft = { title: 'Otra', description: '', sourceBranch: 'feature/pr-101', targetBranch: 'main', draft: false, reviewers: [], workItemIds: [] };
    expect(await pr.create(draft)).toBeNull();
    expect(pr.createError?.code).toBe('CONFLICT');
    expect(pr.creating).toBe(true);
  });

  it('sin plantilla o con error al pedirla, el formulario sigue', async () => {
    const { pr, api } = await prTab();
    vi.spyOn(api, 'pullRequestTemplate').mockRejectedValueOnce(new Error('x'));
    pr.startCreate();
    await vi.waitFor(() => expect(pr.templateLoaded).toBe(true));
    expect(pr.template).toBeNull();
  });
});

describe('ciclo de vida', () => {
  it('prOf devuelve un único controlador por pestaña y release lo vacía', async () => {
    const { tab, pr } = await prTab();
    expect(prOf(tab)).toBe(pr);
    await pr.loadList();
    pr.select(101);
    await vi.waitFor(() => expect(pr.detail).not.toBeNull());
    pr.release();
    expect(pr.items).toEqual([]);
    expect(pr.detail).toBeNull();
    expect(pr.selectedId).toBeNull();
  });

  it('el refresco automático solo admite 0, 60 y 300 s y se recuerda', async () => {
    const { pr } = await prTab();
    pr.setAutoRefresh(10);
    expect(pr.autoRefresh).toBe(0);
    pr.setAutoRefresh(60);
    expect(pr.autoRefresh).toBe(60);
    expect(localStorage.getItem('ariadna.pr.autorefresh.v1')).toBe('60');
  });
});
