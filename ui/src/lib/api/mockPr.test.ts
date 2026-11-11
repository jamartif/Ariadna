import { describe, expect, it } from 'vitest';
import { createMockApi, type MockApi } from './mockApi';
import { ApiError } from './types';
import { check, arr } from './validate';
import { prAction, prDiff, prReviewThread, prSupport, pullRequestDetail, pullRequestPage } from './validatePr';

async function open(url: string, scenario?: string, confirm = true): Promise<{ api: MockApi; tab: string; asked: string[] }> {
  const asked: string[] = [];
  const api = createMockApi({
    latency: 0,
    scenario,
    confirm: (m) => {
      asked.push(m);
      return confirm;
    }
  });
  const tab = (await api.openRepository('/home/javie/proyectos/ariadna')).tab.tabId;
  const repo = api.__repos.get(tab);
  if (repo) repo.remotes = [{ name: 'origin', fetchUrl: url, pushUrl: url }];
  return { api, tab, asked };
}

const GH = 'git@github.com:acme/app.git';
const AZ = 'https://dev.azure.com/org/proj/_git/app';

async function code(p: Promise<unknown>): Promise<string> {
  try {
    await p;
  } catch (e) {
    return e instanceof ApiError ? e.code : String(e);
  }
  return 'OK';
}

describe('backend simulado de pull requests', () => {
  it('soporta Azure DevOps y GitHub; GitLab solo en el navegador', async () => {
    const gh = await open(GH);
    const sup = await gh.api.pullRequestSupport(gh.tab);
    expect(check(prSupport, sup)).toEqual([]);
    expect(sup.supported && sup.provider === 'github').toBe(true);
    expect(sup.capabilities?.votes).toEqual(['approve', 'reject', 'comment']);

    const az = await open(AZ);
    expect((await az.api.pullRequestSupport(az.tab)).capabilities?.strategies).toContain('rebaseMerge');

    const gl = await open('https://gitlab.com/g/r.git');
    const s = await gl.api.pullRequestSupport(gl.tab);
    expect(s.supported).toBe(false);
    expect(s.reason).toMatch(/navegador/);
    expect(await code(gl.api.listPullRequests(gl.tab, 'open', 1))).toBe('POLICY');
  });

  it('lista con filtros y las formas coinciden con el contrato', async () => {
    const { api, tab } = await open(AZ);
    const open_ = await api.listPullRequests(tab, 'open', 1);
    expect(check(pullRequestPage, open_)).toEqual([]);
    expect(open_.items.every((p) => p.state === 'active')).toBe(true);
    expect((await api.listPullRequests(tab, 'mine', 1)).items.map((p) => p.id)).toEqual([102]);
    expect((await api.listPullRequests(tab, 'reviewing', 1)).items.map((p) => p.id)).toEqual([101]);
    expect((await api.listPullRequests(tab, 'closed', 1)).items.map((p) => p.state).sort()).toEqual(['abandoned', 'completed']);
  });

  it('detalle, diff, hilos y plantilla', async () => {
    const { api, tab } = await open(GH);
    const d = await api.getPullRequest(tab, 101);
    expect(check(pullRequestDetail, d)).toEqual([]);
    expect(d.checkoutRef).toBe('refs/pull/101/head');
    expect(d.description).toContain('<script>'); // texto: la UI lo renderiza como Markdown, no como HTML
    expect(check(prDiff, await api.pullRequestDiff(tab, 101))).toEqual([]);
    expect(check(arr(prReviewThread), await api.pullRequestThreads(tab, 101))).toEqual([]);
    expect(await api.pullRequestTemplate(tab)).toMatch(/Qué cambia/);
    expect(await code(api.getPullRequest(tab, 9999))).toBe('NOT_FOUND');
  });

  it('crear: valida, evita duplicadas y devuelve el detalle', async () => {
    const { api, tab } = await open(AZ);
    const draft = { title: ' Nueva ', description: 'd', sourceBranch: 'feature/z', targetBranch: 'main', draft: true, reviewers: ['beto'], workItemIds: [] };
    const d = await api.createPullRequest(tab, draft);
    expect(check(pullRequestDetail, d)).toEqual([]);
    expect(d.title).toBe('Nueva');
    expect(d.isDraft).toBe(true);
    expect(d.checkoutRef).toBe('refs/heads/feature/z');
    expect(await code(api.createPullRequest(tab, draft))).toBe('CONFLICT');
    expect(await code(api.createPullRequest(tab, { ...draft, title: ' ' }))).toBe('INVALID_REFERENCE');
  });

  it('hilos: crear, responder y resolver (GitHub no resuelve ni anida los generales)', async () => {
    const gh = await open(GH);
    const threads = await gh.api.pullRequestThreads(gh.tab, 101);
    const inline = threads.find((t) => t.anchor)!;
    const general = threads.find((t) => !t.anchor)!;
    expect((await gh.api.addPullRequestComment(gh.tab, 101, { kind: 'reply', threadId: inline.id, body: 'ok' })).comments).toHaveLength(3);
    expect((await gh.api.resolvePullRequestThread(gh.tab, 101, inline.id, true)).status).toBe('resolved');
    expect(await code(gh.api.addPullRequestComment(gh.tab, 101, { kind: 'reply', threadId: general.id, body: 'x' }))).toBe('INVALID_REFERENCE');
    expect(await code(gh.api.resolvePullRequestThread(gh.tab, 101, general.id, true))).toBe('INVALID_REFERENCE');
    const created = await gh.api.addPullRequestComment(gh.tab, 101, { kind: 'new', body: 'Mira', anchor: { path: 'a.ts', line: 3, side: 'new' } });
    expect(check(prReviewThread, created)).toEqual([]);
    expect(await code(gh.api.addPullRequestComment(gh.tab, 101, { kind: 'new', body: ' ' }))).toBe('INVALID_REFERENCE');
  });

  it('acciones destructivas: confirmación y rechazo', async () => {
    const no = await open(AZ, undefined, false);
    expect((await no.api.votePullRequest(no.tab, 101, 'reject')).done).toBe(false);
    expect((await no.api.completePullRequest(no.tab, 101, { strategy: 'squash', deleteSourceBranch: true, completeWorkItems: false, autoComplete: false, commitMessage: null })).done).toBe(false);
    expect((await no.api.abandonPullRequest(no.tab, 101)).done).toBe(false);
    expect(no.asked).toHaveLength(3);
    expect(no.asked[0]).toContain('#101');
    expect(no.asked[0]).toContain('main');
    expect((await no.api.getPullRequest(no.tab, 101)).state).toBe('active');
    // Aprobar no pregunta.
    const before = no.asked.length;
    const ok = await no.api.votePullRequest(no.tab, 101, 'approve');
    expect(check(prAction, ok)).toEqual([]);
    expect(no.asked.length).toBe(before);

    const yes = await open(AZ);
    const done = await yes.api.abandonPullRequest(yes.tab, 101);
    expect(done.detail?.state).toBe('abandoned');
    expect(await code(yes.api.abandonPullRequest(yes.tab, 101))).toBe('POLICY');
    expect((await yes.api.reopenPullRequest(yes.tab, 101)).detail?.state).toBe('active');
    expect(await code(yes.api.reopenPullRequest(yes.tab, 103))).toBe('POLICY'); // completada
  });

  it('completar respeta las políticas, los conflictos y las capacidades del proveedor', async () => {
    const gh = await open(GH);
    const opts = { strategy: 'squash' as const, deleteSourceBranch: false, completeWorkItems: false, autoComplete: false, commitMessage: null };
    expect(await code(gh.api.completePullRequest(gh.tab, 101, { ...opts, strategy: 'rebaseMerge' }))).toBe('INVALID_REFERENCE');
    expect(await code(gh.api.completePullRequest(gh.tab, 101, { ...opts, autoComplete: true }))).toBe('INVALID_REFERENCE');
    expect(await code(gh.api.completePullRequest(gh.tab, 105, opts))).toBe('POLICY');
    expect((await gh.api.completePullRequest(gh.tab, 101, opts)).detail?.state).toBe('completed');
    expect(await code(gh.api.votePullRequest(gh.tab, 101, 'waitForAuthor'))).toBe('INVALID_REFERENCE');
    expect(await code(gh.api.votePullRequest(gh.tab, 102, 'reject'))).toBe('INVALID_REFERENCE'); // GitHub exige comentario
  });

  it('escenarios de error: AUTH con el permiso, RATE_LIMITED, NETWORK y vacío', async () => {
    const auth = await open(AZ, 'pr-auth');
    await expect(auth.api.listPullRequests(auth.tab, 'open', 1)).rejects.toMatchObject({ code: 'AUTH', detail: 'pull_requests=read' });
    const rl = await open(AZ, 'pr-rate-limit');
    await expect(rl.api.getPullRequest(rl.tab, 101)).rejects.toMatchObject({ code: 'RATE_LIMITED', detail: '30' });
    const net = await open(AZ, 'pr-network');
    expect(await code(net.api.listPullRequests(net.tab, 'open', 1))).toBe('NETWORK');
    const empty = await open(AZ, 'pr-empty');
    expect((await empty.api.listPullRequests(empty.tab, 'open', 1)).items).toEqual([]);
  });

  it('ningún dato simulado lleva un token', async () => {
    const { api, tab } = await open(GH);
    const all = JSON.stringify([await api.listPullRequests(tab, 'open', 1), await api.getPullRequest(tab, 101), await api.pullRequestThreads(tab, 101)]);
    expect(all).not.toMatch(/ghp_|github_pat_|password|secret|Bearer/i);
  });
});
