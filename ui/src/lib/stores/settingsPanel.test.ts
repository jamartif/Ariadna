import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { settings } from './settings.svelte';
import { SettingsPanel, startSettingsSync } from './settingsPanel.svelte';
import { toasts } from './toasts.svelte';
import { mockApi, resetGlobals } from '../../test/helpers';

beforeEach(resetGlobals);
afterEach(resetGlobals);

async function loaded(opts = {}, tabId: string | null = 'tab-ariadna') {
  const api = mockApi(opts);
  const panel = new SettingsPanel(api, tabId ?? undefined);
  await panel.load();
  await vi.waitFor(() => expect(panel.cli).not.toBeNull());
  await vi.waitFor(() => expect(panel.ai).not.toBeNull());
  return { api, panel };
}

describe('SettingsPanel', () => {
  it('carga ajustes, identidad, IA y estado de los CLI', async () => {
    const { panel } = await loaded();
    expect(panel.draft?.theme).toBe('system');
    expect(panel.view?.about.license).toBe('MIT');
    expect(panel.dirty).toBe(false);
    expect(panel.identity?.name).toBe('Javier Martínez');
    expect(panel.cli?.map((c) => c.id)).toEqual(['claude', 'codex', 'copilot']);
    expect(panel.cli?.find((c) => c.id === 'codex')?.installed).toBe(false);
    expect(panel.ai?.profile.rule).toBeUndefined(); // lo guardado, sin reglas
    expect(panel.effective?.profile).toMatchObject({ kind: 'personal', rule: 'github.com' });
  });

  it('un error de lectura se expone y no deja el borrador a medias', async () => {
    const api = mockApi();
    vi.spyOn(api, 'getSettings').mockRejectedValue(Object.assign(new Error('x'), { code: 'IO', message: 'no se pudo' }));
    const panel = new SettingsPanel(api);
    await panel.load();
    expect(panel.loadError).not.toBeNull();
    expect(panel.draft).toBeNull();
  });

  it('marca cambios, guarda y vuelve a estar limpio', async () => {
    const { api, panel } = await loaded();
    panel.setTheme('dark');
    expect(settings.theme).toBe('dark'); // vista previa inmediata
    panel.draft!.pullRebase = true;
    expect(panel.dirty).toBe(true);
    expect(await panel.save()).toBe(true);
    expect(panel.dirty).toBe(false);
    const stored = (await api.getSettings()).settings;
    expect(stored).toMatchObject({ theme: 'dark', pullRebase: true });
  });

  it('cancelar devuelve el tema y el tamaño a lo guardado', async () => {
    const { panel } = await loaded();
    panel.setTheme('dark');
    panel.setDiffFontSize(18);
    expect(settings.diffFontSize).toBe(18);
    panel.revert();
    expect(settings.theme).toBe('system');
    expect(settings.diffFontSize).toBe(13);
    expect(panel.dirty).toBe(false);
  });

  it('tamaño del diff fuera de rango: no se aplica ni se guarda', async () => {
    const { panel } = await loaded();
    panel.setDiffFontSize(99);
    expect(panel.fontProblem).toBe(true);
    expect(settings.diffFontSize).toBe(13);
    expect(await panel.save()).toBe(false);
    panel.setDiffFontSize(Number.NaN);
    expect(panel.fontProblem).toBe(true);
    panel.setDiffFontSize(16);
    expect(panel.fontProblem).toBe(false);
    expect(settings.diffFontSize).toBe(16);
  });

  it('Detectar rellena la ruta y muestra la versión; una ruta mala da el motivo', async () => {
    const { panel } = await loaded();
    await panel.detectGit();
    expect(panel.gitCheck).toMatchObject({ ok: true, source: 'auto', version: '2.47.0' });
    expect(panel.draft?.gitPath).toBe('/usr/bin/git');
    panel.setGitPath('/nada/de/nada');
    expect(panel.gitCheck).toBeNull();
    await panel.checkGit();
    expect(panel.gitCheck?.ok).toBe(false);
    expect(panel.gitCheck?.problem).toContain('No hay un ejecutable de git');
    panel.setGitPath('   ');
    expect(panel.draft?.gitPath).toBeNull();
  });

  it('guardar con una ruta de git inválida muestra el error de Rust y no cambia nada', async () => {
    const { api, panel } = await loaded();
    panel.setGitPath('/ruta/que/no-existe');
    expect(await panel.save()).toBe(false);
    expect(panel.saveError?.code).toBe('GIT_NOT_FOUND');
    expect((await api.getSettings()).settings.gitPath).toBeNull();
    expect(panel.dirty).toBe(true);
  });

  it('reglas: añade normalizadas, rechaza duplicadas y mal formadas, edita y quita', async () => {
    const { panel } = await loaded();
    expect(panel.addRule('GitLab.com/Acme/', 'empresa')).toBeNull();
    expect(panel.draft?.profileRules.at(-1)).toEqual({ pattern: 'gitlab.com/acme', profile: 'empresa' });
    expect(panel.addRule('gitlab.com/acme', 'personal')).toBe('duplicate');
    expect(panel.addRule('', 'personal')).toBe('empty');
    expect(panel.addRule('https://x.com', 'personal')).toBe('scheme');
    expect(panel.addRule('sin-punto', 'personal')).toBe('host');
    panel.setRuleProfile(0, 'personal');
    expect(panel.draft?.profileRules[0]?.profile).toBe('personal');
    panel.removeRule(1);
    expect(panel.draft?.profileRules.map((r) => r.pattern)).toEqual(['dev.azure.com', 'gitlab.com/acme']);
    expect(await panel.save()).toBe(true);
    expect(panel.effective?.profile.rule).toBe('github.com');
  });

  it('guardar reglas refresca el perfil de la pestaña actual', async () => {
    const { panel } = await loaded();
    panel.removeRule(1); // fuera la regla de github.com
    await panel.save();
    await vi.waitFor(() => expect(panel.effective?.profile.rule).toBeUndefined());
    expect(panel.effective?.profile.kind).toBe('personal');
  });

  it('IA: cambios inmediatos con el estado devuelto por Rust', async () => {
    const { panel } = await loaded({}, null);
    expect(panel.ai?.enabled).toBe(true);
    await panel.patchAi({ enabled: false });
    expect(panel.ai?.enabled).toBe(false);
    await panel.patchAi({ choice: 'codex' });
    expect(panel.ai?.choice).toBe('codex');
    await panel.patchAi({ profile: 'empresa' });
    expect(panel.ai?.profile.kind).toBe('empresa');
    expect(panel.ai?.providers.find((p) => p.id === 'claude')?.enabled).toBe(false);
  });
});

describe('startSettingsSync', () => {
  it('Rust manda al arrancar, avisa si el archivo estaba dañado y guarda los cambios del usuario', async () => {
    const api = mockApi({ scenario: 'settings-corrupt' });
    const s = (await api.getSettings()).settings;
    await api.saveSettings({ ...s, theme: 'dark', diffFontSize: 15 });
    const stop = startSettingsSync(api);
    await vi.waitFor(() => expect(settings.theme).toBe('dark'));
    expect(settings.diffFontSize).toBe(15);
    expect(toasts.items.some((t) => t.kind === 'warning' && /settings\.json\.bak/.test(t.message ?? ''))).toBe(true);

    vi.useFakeTimers();
    settings.setTheme('light');
    await vi.advanceTimersByTimeAsync(300);
    vi.useRealTimers();
    await vi.waitFor(async () => expect((await api.getSettings()).settings.theme).toBe('light'));
    stop();
    expect(settings.onChange).toBeNull();
  });
});
