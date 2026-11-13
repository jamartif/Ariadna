import type { DoneResult } from './types';

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
 * Comandos de etiquetas (`specs/rust/ipc.md`, «Etiquetas»). Errores tipados: `INVALID_TAG_NAME`
 * (nombre no válido, motivo en `message`), `TAG_EXISTS`, `TAG_NOT_FOUND`, `INVALID_REFERENCE`
 * (commit inexistente o remoto no válido), `AUTH` / `COMMAND_FAILED` al publicar.
 * Borrar (local o en el remoto) abre una confirmación nativa en Rust: `{ done: false }` si se cancela.
 */
export interface TagApi {
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
