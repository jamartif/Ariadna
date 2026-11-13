/**
 * Etiquetas del backend simulado (`mockApi`): mismos comandos, formas y errores que Rust, con el
 * estado en memoria. Las etiquetas viven en los `refs` de los commits (`tag: v1`), como las
 * devuelve `git log`, así que crear o borrar una se ve también en el historial.
 */

import { t } from '$lib/i18n';
import { tagNameProblem } from '$lib/util/tagName';
import type { Tag, TagApi } from './tagApi';
import { ApiError, type ApiErrorCode, type Commit, type Remote } from './types';

export interface MockTagDeps {
  /** Repo de la pestaña (lanza `REPO_MISSING` si no existe). Los commits se modifican in situ. */
  repoOf(tabId: string): { name: string; commits: Commit[]; remotes: Remote[] };
  /** Sube la revisión y emite el snapshot, como tras una mutación real. */
  publish(tabId: string): void;
  delay(): Promise<void>;
  /** Doble de la confirmación nativa. */
  confirm(message: string): boolean;
  now(): Date;
}

interface Stored {
  annotated: boolean;
  commit: string;
  message: string | null;
  tagger: string | null;
  dateIso: string;
  /** Remotos donde ya está publicada. */
  published: Set<string>;
}

function err(code: ApiErrorCode, message: string, detail?: string): ApiError {
  return new ApiError({ code, message, detail });
}

const tagRef = (name: string) => `tag: ${name}`;

export function createMockTags(deps: MockTagDeps): TagApi & { __publishedTags(tabId: string): string[] } {
  const stores = new Map<string, Map<string, Stored>>();

  /** Etiquetas del repo; la primera vez salen de los `refs` del historial simulado. */
  function storeOf(tabId: string): Map<string, Stored> {
    let s = stores.get(tabId);
    if (!s) {
      s = new Map();
      for (const c of deps.repoOf(tabId).commits) {
        for (const ref of c.refs) {
          if (!ref.startsWith('tag: ')) continue;
          const name = ref.slice(5);
          // La primera es anotada para tener las dos formas a mano en el simulado.
          const annotated = s.size === 0;
          s.set(name, {
            annotated,
            commit: c.sha,
            message: annotated ? `Versión ${name}\n\nEtiqueta de ejemplo del repositorio simulado.` : null,
            tagger: annotated ? 'Javier' : null,
            dateIso: c.authorDateIso,
            published: new Set()
          });
        }
      }
      stores.set(tabId, s);
    }
    return s;
  }

  const view = (name: string, s: Stored): Tag => ({
    name,
    annotated: s.annotated,
    commit: s.commit,
    message: s.message,
    tagger: s.tagger,
    dateIso: s.dateIso
  });

  const setRef = (commits: Commit[], sha: string, name: string, on: boolean) => {
    const c = commits.find((x) => x.sha === sha);
    if (!c) return;
    c.refs = on ? [...c.refs.filter((r) => r !== tagRef(name)), tagRef(name)] : c.refs.filter((r) => r !== tagRef(name));
  };

  function checkName(name: string): void {
    const problem = tagNameProblem(name);
    if (problem) {
      throw err('INVALID_TAG_NAME', `El nombre de etiqueta «${name}» no es válido: ${t(problem)}`, name);
    }
  }

  return {
    async listTags(tabId) {
      await deps.delay();
      deps.repoOf(tabId);
      return [...storeOf(tabId)].map(([n, s]) => view(n, s)).sort((a, b) => b.dateIso.localeCompare(a.dateIso) || b.name.localeCompare(a.name));
    },

    async createTag(tabId, rawName, target, message) {
      await deps.delay();
      const r = deps.repoOf(tabId);
      const name = rawName.trim();
      checkName(name);
      const store = storeOf(tabId);
      if (store.has(name)) {
        throw err('TAG_EXISTS', `Ya existe una etiqueta llamada «${name}». Elige otro nombre o bórrala antes.`, name);
      }
      const wanted = target?.trim() ? target.trim() : null;
      if (wanted?.startsWith('-')) throw err('INVALID_REFERENCE', `La referencia «${wanted}» no es válida.`, wanted);
      const commit = wanted ? r.commits.find((c) => wanted.length >= 4 && c.sha.startsWith(wanted)) : r.commits[0];
      if (!commit) throw err('INVALID_REFERENCE', `La referencia «${wanted ?? 'HEAD'}» no es válida.`, wanted ?? 'HEAD');
      const note = message?.trim() ? message.trim() : null;
      const stored: Stored = {
        annotated: note !== null,
        commit: commit.sha,
        message: note,
        tagger: note !== null ? 'Javier' : null,
        dateIso: note !== null ? deps.now().toISOString() : commit.authorDateIso,
        published: new Set()
      };
      store.set(name, stored);
      setRef(r.commits, commit.sha, name, true);
      deps.publish(tabId);
      return view(name, stored);
    },

    async deleteTag(tabId, name) {
      await deps.delay();
      const r = deps.repoOf(tabId);
      const store = storeOf(tabId);
      const s = store.get(name);
      if (!s) throw err('TAG_NOT_FOUND', `La etiqueta «${name}» ya no existe. Actualiza la lista e inténtalo de nuevo.`, name);
      const kind = s.annotated ? 'anotada: se pierde su mensaje' : 'ligera';
      if (!deps.confirm(`¿Borrar la etiqueta local «${name}» (${kind}) de ${r.name}? Los commits no se tocan.`)) return { done: false };
      store.delete(name);
      setRef(r.commits, s.commit, name, false);
      deps.publish(tabId);
      return { done: true };
    },

    async pushTag(tabId, remote, name) {
      await deps.delay();
      const r = deps.repoOf(tabId);
      const s = storeOf(tabId).get(name);
      if (!s) throw err('TAG_NOT_FOUND', `La etiqueta «${name}» ya no existe. Actualiza la lista e inténtalo de nuevo.`, name);
      if (remote.startsWith('-') || remote.trim() === '') throw err('INVALID_REFERENCE', `La referencia «${remote}» no es válida.`, remote);
      if (!r.remotes.some((x) => x.name === remote)) {
        throw err('COMMAND_FAILED', `git terminó con código 128.\nfatal: '${remote}' does not appear to be a git repository`, `fatal: '${remote}' does not appear to be a git repository`);
      }
      s.published.add(remote);
      deps.publish(tabId);
    },

    async deleteRemoteTag(tabId, remote, name) {
      await deps.delay();
      const r = deps.repoOf(tabId);
      if (remote.startsWith('-') || remote.trim() === '') throw err('INVALID_REFERENCE', `La referencia «${remote}» no es válida.`, remote);
      if (!deps.confirm(`¿Borrar la etiqueta «${name}» del remoto «${remote}» de ${r.name}? La etiqueta local no se toca.`)) return { done: false };
      if (!r.remotes.some((x) => x.name === remote)) {
        throw err('COMMAND_FAILED', `git terminó con código 128.\nfatal: '${remote}' does not appear to be a git repository`, `fatal: '${remote}' does not appear to be a git repository`);
      }
      storeOf(tabId).get(name)?.published.delete(remote);
      deps.publish(tabId);
      return { done: true };
    },

    /** Solo pruebas: etiquetas publicadas (`remoto/nombre`). */
    __publishedTags(tabId) {
      return [...storeOf(tabId)].flatMap(([n, s]) => [...s.published].map((rm) => `${rm}/${n}`)).sort();
    }
  };
}
