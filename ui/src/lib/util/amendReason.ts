import type { AmendRefusal } from '$lib/api/types';
import { t, type MessageKey } from '$lib/i18n';

/**
 * Motivo que devuelve el motor (`AmendRefusal`, valor de cable en `snake_case`) → clave del
 * catálogo. La correspondencia vive aquí y no en cada componente: añadir un motivo en Rust sin
 * traducirlo rompe el tipo de este mapa.
 */
export const AMEND_REASON_KEYS: Record<AmendRefusal['kind'], MessageKey> = {
  no_commits: 'amendReason.noCommits',
  detached_head: 'amendReason.detachedHead',
  integration_in_progress: 'amendReason.integrationInProgress',
  already_published: 'amendReason.alreadyPublished'
};

/** Texto traducido del motivo; `null` si no hay motivo. Nunca devuelve la clave cruda. */
export function amendRefusalText(reason: AmendRefusal | null | undefined): string | null {
  if (!reason) return null;
  const params = reason.kind === 'already_published' ? { ref: reason.value } : undefined;
  return t(AMEND_REASON_KEYS[reason.kind], params);
}
