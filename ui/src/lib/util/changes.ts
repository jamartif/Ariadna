import type { StatusEntry } from '$lib/api/types';

export type SectionId = 'staged' | 'unstaged' | 'untracked' | 'ignored' | 'conflicts';

export const SECTION_ORDER: SectionId[] = ['conflicts', 'staged', 'unstaged', 'untracked', 'ignored'];

/** Predicados idénticos a `StatusEntry` en Rust (models.rs). */
export function isStaged(e: StatusEntry): boolean {
  return !e.isUntracked && !e.isIgnored && !e.isConflicted && e.indexStatus !== '.';
}

export function isUnstagedTracked(e: StatusEntry): boolean {
  return !e.isUntracked && !e.isIgnored && !e.isConflicted && e.workTreeStatus !== '.';
}

export type Sections = Record<SectionId, StatusEntry[]>;

/** Un archivo puede estar a la vez en Preparados y en Sin preparar. */
export function groupEntries(entries: readonly StatusEntry[]): Sections {
  const out: Sections = { staged: [], unstaged: [], untracked: [], ignored: [], conflicts: [] };
  for (const e of entries) {
    if (e.isConflicted) out.conflicts.push(e);
    else if (e.isIgnored) out.ignored.push(e);
    else if (e.isUntracked) out.untracked.push(e);
    else {
      if (isStaged(e)) out.staged.push(e);
      if (isUnstagedTracked(e)) out.unstaged.push(e);
    }
  }
  return out;
}

/** Letra que se muestra en el icono de estado de la fila. */
export function displayStatus(e: StatusEntry, section: SectionId): string {
  if (section === 'conflicts') return 'U';
  if (section === 'untracked') return '?';
  if (section === 'ignored') return '!';
  const s = section === 'staged' ? e.indexStatus : e.workTreeStatus;
  return s === '' ? '·' : s;
}
