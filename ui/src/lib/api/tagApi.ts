import type { Branch, DoneResult } from './types';

/** Un worktree del repositorio (`git worktree list`). Su `HEAD` puede no estar en ninguna rama. */
export interface Worktree {
  path: string;
  head: string;
  branch: string | null;
  detached: boolean;
  /** El worktree principal (el del propio repositorio). */
  isMain: boolean;
}

/** Lo que el riel necesita para sus grupos «Pull requests» y «Otros worktrees». */
export interface ExtraRefs {
  /**
   * Refs de pull request con su nombre **completo** (`refs/pull/7/head`, `refs/merge-requests/7/head`):
   * el namespace distingue GitHub de GitLab y sirve de clave única en la lista. Sin `refs/*\/merge`.
   */
  pullRefs: Branch[];
  /** Todos los worktrees; el principal va primero (`isMain`). */
  worktrees: Worktree[];
}

/**
 * Una etiqueta del repositorio (`list_tags`, `create_tag`). Forma de `ariadna_git::Tag`.
 */
export interface Tag {
  /** Sin el prefijo `refs/tags/`; puede llevar `/` (`release/1.0`). */
  name: string;
  /** `true` si es anotada (tiene mensaje y autor propios). */
  annotated: boolean;
  /** SHA completo del commit al que apunta (pelado en las anotadas). */
  commit: string;
  /** Mensaje de la anotada (asunto + cuerpo); `null` en una ligera. */
  message: string | null;
  /** Quien creó la anotada; `null` en una ligera. */
  tagger: string | null;
  /** ISO 8601: la fecha de la etiqueta anotada o, en una ligera, la del commit. */
  dateIso: string;
}

/**
 * Comandos de etiquetas (`docs/ipc.md`, «Etiquetas»). Errores tipados: `INVALID_TAG_NAME`
 * (nombre no válido, motivo en `message`), `TAG_EXISTS`, `TAG_NOT_FOUND`, `INVALID_REFERENCE`
 * (commit inexistente o remoto no válido), `AUTH` / `COMMAND_FAILED` al publicar.
 * Borrar (local o en el remoto) abre una confirmación nativa en Rust: `{ done: false }` si se cancela.
 */
export interface TagApi {
  /**
   * Referencias que el riel pinta aparte del snapshot diario: refs de pull request
   * (`refs/pull/*`, `refs/merge-requests/*`) y worktrees del repo.
   */
  extraRefs(tabId: string): Promise<ExtraRefs>;
  /** De la más reciente a la más antigua. */
  listTags(tabId: string): Promise<Tag[]>;
  /** En `target` (SHA; `null` = HEAD). Con `message` no vacío es anotada. */
  createTag(tabId: string, name: string, target: string | null, message: string | null): Promise<Tag>;
  /** Solo la etiqueta local. Confirmación nativa. */
  deleteTag(tabId: string, name: string): Promise<DoneResult>;
  /** `git push <remote> refs/tags/<name>`. Git rechaza pisar una etiqueta remota distinta. */
  pushTag(tabId: string, remote: string, name: string): Promise<void>;
  /** Borra la etiqueta del servidor (no la local). Confirmación nativa. */
  deleteRemoteTag(tabId: string, remote: string, name: string): Promise<DoneResult>;
}
