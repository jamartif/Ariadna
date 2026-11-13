import { invoke } from '@tauri-apps/api/core';
import type { TagApi } from './tagApi';
import { toApiError } from './types';

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    throw toApiError(e);
  }
}

/** Comandos de etiquetas sobre `invoke`. Argumentos en camelCase (ipc.md). */
export function createTauriTags(): TagApi {
  return {
    listTags: (tabId) => call('list_tags', { tabId }),
    createTag: (tabId, name, target, message) => call('create_tag', { tabId, name, target, message }),
    deleteTag: (tabId, name) => call('delete_tag', { tabId, name }),
    pushTag: (tabId, remote, name) => call('push_tag', { tabId, remote, name }),
    deleteRemoteTag: (tabId, remote, name) => call('delete_remote_tag', { tabId, remote, name })
  };
}
