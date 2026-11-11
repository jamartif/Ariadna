import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Api } from './api';
import { createTauriPullRequests } from './tauriPr';
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
    openExternalUrl: (url) => call('open_external_url', { url }),
    // El PAT viaja una sola vez hacia Rust (entorno del proceso git vía GIT_ASKPASS temporal); no se guarda.
    cloneRepository: (url, destination, branch, auth) =>
      call('clone_repository', { url, destination, branch, username: auth?.username ?? null, secret: auth?.secret ?? null }),
    // El secreto viaja una sola vez hacia Rust (stdin de `git credential approve`) y no se devuelve.
    storeCredential: (url, username, secret) => call('store_credential', { url, username, secret }),
    testCredential: (url) => call('test_credential', { url }),
    forgetCredential: (url, username) => call('forget_credential', { url, username }),

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
    logPage: (tabId, skip, limit, query) => call('log_page', { tabId, skip, limit, query }),
    createBranch: (tabId, name, startPoint) => call('create_branch', { tabId, name, startPoint }),
    renameBranch: (tabId, oldName, newName) =>
      call('rename_branch', { tabId, old: oldName, new: newName }),
    checkout: (tabId, reference) => call('checkout', { tabId, reference }),
    deleteBranch: (tabId, name, force) => call('delete_branch', { tabId, name, force }),
    merge: (tabId, branch, fastForward) => call('merge', { tabId, branch, fastForward }),
    mergeClean: (tabId, branch, fastForward) => call('merge_clean', { tabId, branch, fastForward }),
    continueIntegration: (tabId) => call('continue_integration', { tabId }),
    abortIntegration: (tabId) => call('abort_integration', { tabId }),
    mergeStart: (tabId, branch, fastForward) => call('merge_start', { tabId, branch, fastForward }),
    mergeState: (tabId) => call('merge_state', { tabId }),
    resolveConflict: (tabId, path, resolution) => call('resolve_conflict', { tabId, path, resolution }),
    mergeContinue: (tabId, message) => call('merge_continue', { tabId, message }),
    mergeAbort: (tabId) => call('merge_abort', { tabId }),
    getConflicts: (tabId) => call('get_conflicts', { tabId }),
    writeConflictResolution: (tabId, path, contents) =>
      call('write_conflict_resolution', { tabId, path, contents }),
    markResolved: (tabId, path) => call('mark_resolved', { tabId, path }),
    fetch: (tabId, remote) => call('fetch', { tabId, remote }),
    pull: (tabId, remote, refspec, rebase) => call('pull', { tabId, remote, refspec, rebase: rebase ?? null }),

    listRemotes: (tabId) => call('list_remotes', { tabId }),
    addRemote: (tabId, name, url) => call('add_remote', { tabId, name, url }),
    renameRemote: (tabId, oldName, newName) => call('rename_remote', { tabId, old: oldName, new: newName }),
    removeRemote: (tabId, name) => call('remove_remote', { tabId, name }),
    openRemoteUrl: (tabId, remote, kind, branch) => call('open_remote_url', { tabId, remote, kind, branch }),

    getSettings: () => call('get_settings'),
    saveSettings: (settings) => call('save_settings', { settings }),
    detectGit: (path) => call('detect_git', { path }),
    gitIdentity: () => call('git_identity'),
    getCliStatus: () => call('get_cli_status'),
    push: (tabId, remote, refspec, setUpstream) =>
      call('push', { tabId, remote, refspec, setUpstream }),

    aiState: (tabId) => call('ai_state', { tabId }),
    aiUpdateSettings: (patch) => call('ai_update_settings', { patch }),
    aiPreviewContext: (tabId) => call('ai_preview_context', { tabId }),
    aiGenerateCommitMessage: (tabId, options) => call('ai_generate_commit_message', { tabId, options }),
    aiCancel: (tabId) => call('ai_cancel', { tabId }),
    aiAccounts: (request) => call('ai_accounts', { request: request ?? null }),
    aiLogin: (request) => call('ai_login', { request }),
    aiGeneratePullRequest: (tabId, options) => call('ai_generate_pull_request', { tabId, options: options ?? null }),

    ...createTauriPullRequests(),

    async listen(event, handler) {
      const un = await listen<never>(event, (e) => handler(e.payload as never));
      return un;
    }
  };
}
