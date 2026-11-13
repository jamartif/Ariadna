import { invoke } from '@tauri-apps/api/core';
import type { PullRequestApi } from './prApi';
import { toApiError } from './types';

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    throw toApiError(e);
  }
}

/** Comandos de pull request sobre `invoke`. Argumentos en camelCase (ipc.md). Ninguno lleva token. */
export function createTauriPullRequests(): PullRequestApi {
  return {
    pullRequestSupport: (tabId) => call('pull_request_support', { tabId }),
    listPullRequests: (tabId, filter, page) => call('list_pull_requests', { tabId, filter, page }),
    getPullRequest: (tabId, id) => call('get_pull_request', { tabId, id }),
    checkoutPullRequest: (tabId, id) => call('checkout_pull_request', { tabId, id }),
    createPullRequest: (tabId, draft) => call('create_pull_request', { tabId, draft }),
    pullRequestTemplate: (tabId) => call('pull_request_template', { tabId }),
    pullRequestDiff: (tabId, id) => call('pull_request_diff', { tabId, id }),
    pullRequestThreads: (tabId, id) => call('pull_request_threads', { tabId, id }),
    addPullRequestComment: (tabId, id, thread) => call('add_pull_request_comment', { tabId, id, thread }),
    resolvePullRequestThread: (tabId, id, threadId, resolved) =>
      call('resolve_pull_request_thread', { tabId, id, threadId, resolved }),
    votePullRequest: (tabId, id, vote, comment) => call('vote_pull_request', { tabId, id, vote, comment: comment ?? null }),
    completePullRequest: (tabId, id, options) => call('complete_pull_request', { tabId, id, options }),
    abandonPullRequest: (tabId, id) => call('abandon_pull_request', { tabId, id }),
    reopenPullRequest: (tabId, id) => call('reopen_pull_request', { tabId, id })
  };
}
