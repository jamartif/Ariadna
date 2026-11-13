import { describe, expect, it } from 'vitest';
import { createMockApi } from './mockApi';
import { aiAccounts, aiConflictSuggestion, aiLoginInfo, aiPullRequestSuggestion, aiState, check, detect } from './validate';

const mk = (scenario?: string) => createMockApi({ latency: 0, confirm: () => true, ...(scenario ? { scenario } : {}) });

describe('contrato IA alpha.2: formas', () => {
  it('el estado lleva el aislamiento por proveedor y perfil', async () => {
    const api = mk();
    const st = await api.aiState('tab-ariadna');
    expect(check(aiState, st)).toEqual([]);
    expect(st.profile.isolated).toBe(false); // Personal conserva la sesión normal
    const by = (id: string) => st.providers.find((p) => p.id === id)!;
    expect(by('copilot').sharesLogin).toBe(true);
    expect(by('claude')).toMatchObject({ sharesLogin: false, isolated: false, isolationVerified: true });
    expect(by('codex').isolationVerified).toBe(false);

    const empresa = await api.aiUpdateSettings({ profile: 'empresa' });
    expect(empresa.profile.isolated).toBe(true);
    expect(empresa.providers.find((p) => p.id === 'codex')!.isolated).toBe(true);
    // Copilot no se puede aislar: aunque el perfil aísle, comparte login.
    expect(empresa.providers.find((p) => p.id === 'copilot')).toMatchObject({ sharesLogin: true, isolated: false });

    const off = await api.aiUpdateSettings({ isolated: false });
    expect(off.profile.isolated).toBe(false);
    expect(off.providers.every((p) => !p.isolated)).toBe(true);
  });

  it('ai_accounts: cuenta del CLI con su directorio; lo que no se expone es «no disponible»', async () => {
    const api = mk();
    const personal = await api.aiAccounts({ profile: 'personal' });
    expect(check(aiAccounts, personal)).toEqual([]);
    expect(personal.accounts.map((a) => [a.provider, a.state])).toEqual([
      ['claude', 'loggedIn'],
      ['codex', 'unavailable'],
      ['copilot', 'unavailable']
    ]);
    expect(personal.accounts[0]).toMatchObject({ email: 'javier@example.com', organization: null, configDir: '/home/javier/.claude' });
    const empresa = await api.aiAccounts({ profile: 'empresa' });
    expect(empresa.accounts[0]).toMatchObject({ email: 'javier@miempresa.com', organization: 'Mi Empresa SL' });
    expect(empresa.accounts[0]!.configDir).toContain('/ai-profiles/empresa/claude');
    expect(empresa.accounts[1]!.reason).toMatch(/no expone/);
    // Sin petición: el perfil activo.
    expect((await api.aiAccounts()).profile.kind).toBe('personal');
  });

  it('ai_accounts: sin sesión y CLI ausente', async () => {
    expect((await mk('ai-not-logged').aiAccounts({ profile: 'personal' })).accounts[0]!.state).toBe('loggedOut');
    expect((await mk('ai-not-installed').aiAccounts({ profile: 'personal' })).accounts[0]).toMatchObject({ state: 'unavailable' });
  });

  it('ai_login: comando aislado en Empresa, normal en Personal y aviso en Copilot', async () => {
    const api = mk();
    await api.aiUpdateSettings({ profile: 'empresa', providerEnabled: { id: 'claude', enabled: true } });
    const empresa = await api.aiLogin({ provider: 'claude', profile: 'empresa' });
    expect(check(aiLoginInfo, empresa)).toEqual([]);
    expect(empresa).toMatchObject({ isolated: true, sharesLogin: false });
    expect(empresa.command).toMatch(/^CLAUDE_CONFIG_DIR='.*\/ai-profiles\/empresa\/claude' claude auth login$/);
    expect(empresa.configDir).toContain('/ai-profiles/empresa/claude');
    const personal = await api.aiLogin({ provider: 'claude', profile: 'personal' });
    expect(check(aiLoginInfo, personal)).toEqual([]);
    expect(personal).toMatchObject({ command: 'claude auth login', configDir: null, isolated: false, launched: false, verified: true });
    expect(personal.note).toMatch(/sesión normal/);

    const cp = await api.aiLogin({ provider: 'copilot', profile: 'personal', launch: true });
    expect(cp).toMatchObject({ sharesLogin: true, isolated: false, configDir: null, launched: true, verified: false });
    expect(cp.note).toMatch(/comparten el mismo login/);
    expect(api.__aiLogins().map((l) => l.provider)).toEqual(expect.arrayContaining(['claude', 'copilot']));
  });

  it('ai_login: errores claros y terminal que no se puede abrir', async () => {
    const api = mk();
    await expect(api.aiLogin({ provider: 'codex', profile: 'personal' })).rejects.toMatchObject({ code: 'AI_NOT_INSTALLED' });
    await expect(api.aiLogin({ provider: 'gemini', profile: 'personal' })).rejects.toMatchObject({ code: 'PARSE' });
    const sin = await mk('ai-no-terminal').aiLogin({ provider: 'claude', profile: 'personal', launch: true });
    expect(sin.launched).toBe(false);
    expect(sin.note).toMatch(/No se pudo abrir la terminal/);
  });

  it('ai_generate_pull_request: solo propone, con la forma del contrato', async () => {
    const api = mk();
    const s = await api.aiGeneratePullRequest('tab-ariadna', { base: 'develop' });
    expect(check(aiPullRequestSuggestion, s)).toEqual([]);
    expect(detect(s)?.name).toBe('AiPullRequestSuggestion');
    expect(s.title.length).toBeLessThanOrEqual(72);
    expect(s.base).toBe('develop');
    expect(s.body).toContain('## Resumen');
    await expect(api.aiGeneratePullRequest('tab-ariadna', { base: 'no-existe' })).rejects.toMatchObject({ code: 'AI_BASE_NOT_FOUND' });
    await expect(mk('ai-consent-denied').aiGeneratePullRequest('tab-ariadna')).rejects.toMatchObject({ code: 'AI_CONSENT_REQUIRED' });
    await api.aiUpdateSettings({ enabled: false });
    await expect(api.aiGeneratePullRequest('tab-ariadna')).rejects.toMatchObject({ code: 'AI_DISABLED' });
  });

  it('ai_resolve_conflict: propone el archivo completo, sin marcadores y sin escribirlo', async () => {
    const api = mk();
    const s = await api.aiResolveConflict('tab-legacy', 'src/billing/invoice.ts');
    expect(check(aiConflictSuggestion, s)).toEqual([]);
    expect(detect(s)?.name).toBe('AiConflictSuggestion');
    expect(s.path).toBe('src/billing/invoice.ts');
    // Archivo completo: sin marcadores y con las dos versiones dentro.
    expect(s.content).not.toMatch(/<<<<<<<|=======|>>>>>>>/);
    expect(s.content).toContain('applyDiscount');
    expect(s.content).toContain('taxFor');
    expect(s.rationale).toMatch(/Se conservan los dos cambios/);

    // Nada resuelto: el archivo sigue en conflicto con su contenido original.
    const st = await api.mergeState('tab-legacy');
    expect(st.conflicts).toHaveLength(4);
    const file = (await api.getConflicts('tab-legacy')).find((f) => f.path === 'src/billing/invoice.ts')!;
    expect(file.workingTree).toContain('<<<<<<<');
  });

  it('ai_resolve_conflict: errores tipados (binario, no existe, propuesta inválida, IA apagada)', async () => {
    const api = mk();
    await expect(api.aiResolveConflict('tab-legacy', 'assets/logo.png')).rejects.toMatchObject({ code: 'AI_CONFLICT_NOT_TEXT' });
    await expect(api.aiResolveConflict('tab-legacy', 'README.md')).rejects.toMatchObject({ code: 'CONFLICT_NOT_FOUND' });
    await expect(mk('ai-conflict-markers').aiResolveConflict('tab-legacy', 'src/billing/invoice.ts')).rejects.toMatchObject({
      code: 'AI_CONFLICT_INVALID_PROPOSAL'
    });
    await expect(mk('ai-consent-denied').aiResolveConflict('tab-legacy', 'src/billing/invoice.ts')).rejects.toMatchObject({
      code: 'AI_CONSENT_REQUIRED'
    });
    await api.aiUpdateSettings({ enabled: false });
    await expect(api.aiResolveConflict('tab-legacy', 'src/billing/invoice.ts')).rejects.toMatchObject({ code: 'AI_DISABLED' });
  });
});
