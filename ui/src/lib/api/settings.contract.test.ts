import { describe, expect, it } from 'vitest';
import { createMockApi } from './mockApi';
import { appSettings, check, cliStatus, gitDetect, gitIdentity, settingsView } from './validate';

describe('contrato: Ajustes (mock = formas de ipc.md)', () => {
  it('get_settings, detect_git, git_identity y get_cli_status validan', async () => {
    const api = createMockApi({ latency: 0 });
    expect(check(settingsView, await api.getSettings())).toEqual([]);
    expect(check(gitDetect, await api.detectGit(null))).toEqual([]);
    expect(check(gitDetect, await api.detectGit('/no/hay/nada'))).toEqual([]);
    expect(check(gitIdentity, await api.gitIdentity())).toEqual([]);
    const cli = await api.getCliStatus();
    expect(cli).toHaveLength(3);
    for (const c of cli) expect(check(cliStatus, c)).toEqual([]);
  });

  it('los validadores detectan claves inventadas y valores fuera de rango', () => {
    const base = { version: 1, language: 'es', theme: 'dark', diffFontSize: 13, gitPath: null, pullRebase: false, profileRules: [], oauthGitHubClientId: null, oauthGitLabClientId: 'Iv1.abc' };
    expect(check(appSettings, base)).toEqual([]);
    expect(check(appSettings, { ...base, theme: 'fucsia' }).join()).toMatch(/fuera de/);
    expect(check(appSettings, { ...base, extra: 1 }).join()).toMatch(/clave desconocida/);
    expect(check(appSettings, { ...base, diffFontSize: 13.5 }).join()).toMatch(/entero/);
  });

  it('el mock valida igual que Rust: rango del texto, ruta de git y patrones', async () => {
    const api = createMockApi({ latency: 0 });
    const s = (await api.getSettings()).settings;
    await expect(api.saveSettings({ ...s, diffFontSize: 3 })).rejects.toMatchObject({ code: 'PARSE', detail: 'diffFontSize' });
    await expect(api.saveSettings({ ...s, profileRules: [{ pattern: 'https://x.com', profile: 'personal' }] })).rejects.toMatchObject({ code: 'PARSE' });
    await expect(api.saveSettings({ ...s, gitPath: '/ruta/que/no-existe' })).rejects.toMatchObject({ code: 'GIT_NOT_FOUND' });
    const ok = await api.saveSettings({ ...s, gitPath: '/opt/git/bin/git', pullRebase: true, profileRules: [{ pattern: 'GitLab.com/Acme/', profile: 'empresa' }] });
    expect(ok.git?.path).toBe('/opt/git/bin/git');
    expect(ok.settings.profileRules[0]?.pattern).toBe('gitlab.com/acme');
    expect(check(settingsView, ok)).toEqual([]);
  });

  it('pull sin indicación usa el ajuste; con indicación manda ella', async () => {
    const api = createMockApi({ latency: 0 });
    await api.pull('tab-ariadna', 'origin', null);
    expect(api.__lastPullRebase()).toBe(false);
    const s = (await api.getSettings()).settings;
    await api.saveSettings({ ...s, pullRebase: true });
    await api.pull('tab-ariadna', 'origin', null);
    expect(api.__lastPullRebase()).toBe(true);
    await api.pull('tab-ariadna', 'origin', null, false);
    expect(api.__lastPullRebase()).toBe(false);
  });

  it('la regla repo → perfil manda en aiState y se indica', async () => {
    const api = createMockApi({ latency: 0 });
    // El repo de ejemplo apunta a github.com → Personal por la regla por defecto.
    const st = await api.aiState('tab-ariadna');
    expect(st.profile).toMatchObject({ kind: 'personal', rule: 'github.com' });
    const s = (await api.getSettings()).settings;
    await api.saveSettings({ ...s, profileRules: [{ pattern: 'github.com/javie', profile: 'empresa' }] });
    const st2 = await api.aiState('tab-ariadna');
    expect(st2.profile).toMatchObject({ kind: 'empresa', rule: 'github.com/javie' });
    expect(st2.providers.find((p) => p.id === 'claude')?.enabled).toBe(false);
  });
});
