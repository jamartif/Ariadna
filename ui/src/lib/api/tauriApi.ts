import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Api } from './api';
import { toApiError } from './types';

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    throw toApiError(e);
  }
}

/** Implementación real: los argumentos van en camelCase (ipc.md). */
export function createTauriApi(): Api {
  return {
    appState: () => call('app_state'),
    pickFolder: () => call('pick_folder'),
    openRepository: (path) => call('open_repository', { path }),
    closeTab: (tabId) => call('close_tab', { tabId }),
    activateTab: (tabId) => call('activate_tab', { tabId }),
    reorderTabs: (tabIds) => call('reorder_tabs', { tabIds }),
    toggleFavorite: (path) => call('toggle_favorite', { path }),
    forgetRecent: (path) => call('forget_recent', { path }),
    cancelOperation: (tabId, operationId) => call('cancel_operation', { tabId, operationId }),

    getSnapshot: (tabId) => call('get_snapshot', { tabId }),
    refresh: (tabId) => call('refresh', { tabId }),
    stage: (tabId, paths) => call('stage', { tabId, paths }),
    unstage: (tabId, paths) => call('unstage', { tabId, paths }),
    stageHunk: (tabId, path, hunk) => call('stage_hunk', { tabId, path, hunk }),
    unstageHunk: (tabId, path, hunk) => call('unstage_hunk', { tabId, path, hunk }),
    discard: (tabId, paths) => call('discard', { tabId, paths }),
    commit: (tabId, message, allowDetached) => call('commit', { tabId, message, allowDetached }),
    amend: (tabId, message) => call('amend', { tabId, message }),
    amendDraft: (tabId) => call('amend_draft', { tabId }),
    diff: (tabId, scope, paths) => call('diff', { tabId, scope, paths }),
    log: (tabId, limit, query) => call('log', { tabId, limit, query }),
    createBranch: (tabId, name, startPoint) => call('create_branch', { tabId, name, startPoint }),
    renameBranch: (tabId, oldName, newName) =>
      call('rename_branch', { tabId, old: oldName, new: newName }),
    checkout: (tabId, reference) => call('checkout', { tabId, reference }),
    deleteBranch: (tabId, name, force) => call('delete_branch', { tabId, name, force }),
    merge: (tabId, branch, fastForward) => call('merge', { tabId, branch, fastForward }),
    continueIntegration: (tabId) => call('continue_integration', { tabId }),
    abortIntegration: (tabId) => call('abort_integration', { tabId }),
    getConflicts: (tabId) => call('get_conflicts', { tabId }),
    writeConflictResolution: (tabId, path, contents) =>
      call('write_conflict_resolution', { tabId, path, contents }),
    markResolved: (tabId, path) => call('mark_resolved', { tabId, path }),
    fetch: (tabId, remote) => call('fetch', { tabId, remote }),
    pull: (tabId, remote, refspec, rebase) => call('pull', { tabId, remote, refspec, rebase }),
    push: (tabId, remote, refspec, setUpstream) =>
      call('push', { tabId, remote, refspec, setUpstream }),

    async listen(event, handler) {
      const un = await listen<never>(event, (e) => handler(e.payload as never));
      return un;
    }
  };
}
