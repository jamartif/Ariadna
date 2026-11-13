import { t, tn, type MessageKey } from '$lib/i18n';
import { SHORTCUTS, formatCombo, isMac, runShortcut } from '$lib/shortcuts/registry';
import type { AppStore } from './app.svelte';
import { settings } from './settings.svelte';
import { prOf } from './pullRequests.svelte';
import { stashOf } from './stash.svelte';
import { settingsUi } from './settingsPanel.svelte';

export interface PaletteAction {
  id: string;
  label: string;
  group: string;
  /** Texto del atajo ya formateado para la plataforma. */
  shortcut?: string;
  run: () => void;
}

function combo(id: string): string | undefined {
  const def = SHORTCUTS.find((s) => s.id === id);
  return def ? formatCombo(def.combo, isMac()) : undefined;
}

/**
 * Acciones de la paleta (Ctrl/⌘+K). Las que tienen atajo lo ejecutan a través del
 * registro único; el resto llaman directamente a los stores.
 */
export function buildActions(app: AppStore): PaletteAction[] {
  const actions: PaletteAction[] = [];
  const g = (k: MessageKey) => t(k);
  const add = (a: PaletteAction) => actions.push(a);

  add({ id: 'tab.new', label: t('shortcut.newTab'), group: g('palette.group.tabs'), shortcut: combo('tab.new'), run: () => app.newTab() });
  add({
    id: 'tab.close',
    label: t('shortcut.closeTab'),
    group: g('palette.group.tabs'),
    shortcut: combo('tab.close'),
    run: () => {
      if (app.activeId) void app.close(app.activeId);
    }
  });
  if (app.entries.length > 1) {
    add({ id: 'tab.next', label: t('shortcut.nextTab'), group: g('palette.group.tabs'), shortcut: combo('tab.next'), run: () => app.step(1) });
    add({ id: 'tab.prev', label: t('shortcut.prevTab'), group: g('palette.group.tabs'), shortcut: combo('tab.prev'), run: () => app.step(-1) });
  }
  app.entries.forEach((e, i) => {
    if (e.id === app.activeId) return;
    const name = e.kind === 'new' ? t('tabs.newTab') : e.info.name;
    add({
      id: `tab.goto.${e.id}`,
      label: t('palette.gotoTab', { name }),
      group: g('palette.group.tabs'),
      shortcut: i < 9 ? combo(`tab.goto.${i + 1}`) : undefined,
      run: () => app.activate(e.id)
    });
  });

  const tab = app.activeTab;
  if (tab && app.activeEntry?.kind === 'repo') {
    const g2 = g('palette.group.repo');
    add({ id: 'changes.refresh', label: t('shortcut.refresh'), group: g2, shortcut: combo('changes.refresh'), run: () => void tab.refresh() });
    add({
      id: 'changes.stageAll',
      label: t('shortcut.stageAll'),
      group: g2,
      shortcut: combo('changes.stageAll'),
      run: () => void tab.stageAll()
    });
    add({
      id: 'changes.unstageAll',
      label: t('shortcut.unstageAll'),
      group: g2,
      shortcut: combo('changes.unstageAll'),
      run: () => void tab.unstageAll()
    });
    add({ id: 'commit.focus', label: t('shortcut.focusCommit'), group: g2, shortcut: combo('commit.focus'), run: () => runShortcut('commit.focus') });
    add({ id: 'panel.changes', label: t('palette.showChanges'), group: g2, run: () => (tab.panel = 'changes') });
    add({ id: 'panel.history', label: t('palette.showHistory'), group: g2, run: () => (tab.panel = 'history') });
    add({
      id: 'stash.push',
      label: t('palette.stashPush'),
      group: g2,
      shortcut: combo('stash.push'),
      run: () => {
        tab.panel = 'changes';
        stashOf(tab).openPush();
      }
    });
    add({ id: 'stash.open', label: t('palette.stashOpen'), group: g2, shortcut: combo('stash.open'), run: () => (tab.panel = 'stashes') });
    const latest = stashOf(tab).list[0];
    if (latest) {
      add({
        id: 'stash.popLatest',
        label: `${t('palette.stashPop')}: ${latest.message}`,
        group: g2,
        run: () => void stashOf(tab).pop(latest.oid)
      });
    }
    add({ id: 'pr.open', label: t('shortcut.pullRequests'), group: g2, shortcut: combo('pr.open'), run: () => (tab.panel = 'pullRequests') });
    if (prOf(tab).supported) {
      add({
        id: 'pr.new',
        label: t('palette.newPullRequest'),
        group: g2,
        run: () => {
          tab.panel = 'pullRequests';
          prOf(tab).startCreate();
        }
      });
    }
    add({
      id: 'changes.toggleIgnored',
      label: tab.showIgnored ? t('palette.hideIgnored') : t('palette.showIgnored'),
      group: g2,
      run: () => (tab.showIgnored = !tab.showIgnored)
    });
    if (tab.stagedCount > 0) {
      add({ id: 'info.staged', label: tn('commit.staged', tab.stagedCount), group: g2, run: () => (tab.panel = 'changes') });
    }
  }

  add({ id: 'repo.open', label: t('shortcut.openRepo'), group: g('palette.group.repo'), shortcut: combo('repo.open'), run: () => void app.openViaDialog() });
  add({ id: 'clone.open', label: t('shortcut.clone'), group: g('palette.group.repo'), shortcut: combo('clone.open'), run: () => app.openClone() });
  if (tab && app.activeEntry?.kind === 'repo') {
    add({
      id: 'ai.generate',
      label: t('shortcut.aiGenerate'),
      group: g('palette.group.repo'),
      shortcut: combo('ai.generate'),
      run: () => runShortcut('ai.generate')
    });
  }

  add({ id: 'settings.open', label: t('shortcut.settings'), group: g('palette.group.view'), shortcut: combo('settings.open'), run: () => settingsUi.open() });
  const g3 = g('palette.group.view');
  add({ id: 'theme.system', label: t('palette.themeSystem'), group: g3, run: () => settings.setTheme('system') });
  add({ id: 'theme.light', label: t('palette.themeLight'), group: g3, run: () => settings.setTheme('light') });
  add({ id: 'theme.dark', label: t('palette.themeDark'), group: g3, run: () => settings.setTheme('dark') });
  add({
    id: 'density.toggle',
    label: settings.density === 'compact' ? t('palette.densityComfortable') : t('palette.densityCompact'),
    group: g3,
    run: () => settings.toggleDensity()
  });

  const g4 = g('palette.group.recents');
  for (const r of app.recents.slice(0, 8)) {
    add({
      id: `recent.${r.path}`,
      label: t('palette.openRecent', { name: r.name }),
      group: g4,
      run: () => {
        const draft = app.activeEntry?.kind === 'new' ? app.activeEntry.id : undefined;
        void app.openPath(r.path, draft);
      }
    });
  }
  return actions;
}
