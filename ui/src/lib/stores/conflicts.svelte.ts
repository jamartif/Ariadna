import type { ApiError, ConflictChoice, ConflictFile, GitPath, MergeState } from '$lib/api/types';
import { toApiError } from '$lib/api/types';
import { t, tn } from '$lib/i18n';
import { dominantEol, hasMarkers, parseMarkers, resolveAllRegions, resolveRegion, withEol, type HunkChoiceSide } from '$lib/util/conflictMarkers';
import { pathKey, pathText } from '$lib/util/gitpath';
import { toasts } from './toasts.svelte';
import type { TabState } from './tab.svelte';

/** Edición en curso de un archivo: el texto del resultado y el que había en disco al empezar. */
interface Draft {
  text: string;
  original: string | null;
}

export type FileMode = 'editor' | 'sides';

/**
 * Propuesta de la IA para un archivo en conflicto: el archivo **completo** ya resuelto y su
 * justificación. Vive en memoria; aplicarla solo cambia el texto del editor («Usar»), nunca el
 * disco: eso sigue siendo cosa de «Marcar como resuelto».
 */
export interface ConflictAiProposal {
  /** Ruta tal como la devolvió Rust. */
  path: GitPath;
  content: string;
  rationale: string | null;
  provider: string;
  commandSummary: string;
  repoRevision: number;
}

const encoder = new TextEncoder();

/**
 * Estudio de conflictos de UNA pestaña. La verdad vive en Rust (índice con etapas 1/2/3 y el
 * `Snapshot`); aquí solo hay la vista y las ediciones **sin aplicar** del texto resultado.
 * Nada se escribe en disco hasta «Marcar como resuelto» (Rust pide confirmación nativa).
 */
export class ConflictsController {
  state = $state.raw<MergeState | null>(null);
  selectedKey = $state<string | null>(null);
  busy = $state<string | null>(null);
  drafts = $state.raw<ReadonlyMap<string, Draft>>(new Map());
  /** Rutas resueltas desde que empezó la integración (para el progreso). */
  resolved = $state.raw<readonly string[]>([]);
  /** Mensaje del commit de merge (parte del mensaje por defecto de git; editable). */
  message = $state('');
  /** Índice (0-based) del bloque actual del resultado. */
  currentHunk = $state(0);
  /** Cambia cada vez que hay que llevar el cursor del resultado al bloque actual. */
  revealTick = $state(0);
  /** Aviso previo a abortar con ediciones sin aplicar. */
  abortWarning = $state(false);

  /** Propuesta de la IA pendiente de revisar (null = ninguna). */
  aiProposal = $state.raw<ConflictAiProposal | null>(null);
  /** Se está pidiendo la propuesta (o regenerándola). */
  aiLoading = $state(false);
  aiRegenerating = $state(false);
  /** Último error de la IA: se muestra en el editor o dentro del panel. */
  aiError = $state.raw<ApiError | null>(null);
  private aiRun = 0;

  private messageTouched = false;
  private syncSeq = 0;

  constructor(readonly tab: TabState) {}

  // ---- Datos derivados -------------------------------------------------------------------

  get integration() {
    return this.tab.snapshot?.integration ?? 'none';
  }

  /** Hay un merge/rebase/cherry-pick en curso, o archivos en conflicto sin integración (p. ej. tras aplicar un stash). */
  get active(): boolean {
    return this.integration !== 'none' || this.files.length > 0;
  }

  /** Conflictos sin integración en curso: no hay nada que continuar ni abortar. */
  get standalone(): boolean {
    return this.integration === 'none' && this.files.length > 0;
  }

  get files(): ConflictFile[] {
    return this.tab.snapshot?.conflicts ?? [];
  }

  get count(): number {
    return this.files.length;
  }

  /** Total de archivos que hubo en conflicto desde que empezó (resueltos + pendientes). */
  get total(): number {
    return this.resolved.length + this.files.length;
  }

  get selected(): ConflictFile | null {
    const key = this.selectedKey;
    return this.files.find((f) => pathKey(f.path) === key) ?? null;
  }

  get canContinue(): boolean {
    return this.active && this.files.length === 0 && this.busy === null;
  }

  get sourceLabel(): string | null {
    return this.state?.sourceLabel ?? null;
  }

  isBinary(f: ConflictFile): boolean {
    const summary = this.state?.conflicts.find((c) => pathKey(c.path) === pathKey(f.path));
    return summary?.binary ?? (f.ours.isBinary || f.base.isBinary || f.theirs.isBinary);
  }

  /** ¿Se puede editar el texto sin alterar bytes? (UTF-8 y no binario). */
  isTextSafe(f: ConflictFile): boolean {
    const summary = this.state?.conflicts.find((c) => pathKey(c.path) === pathKey(f.path));
    if (summary) return summary.textSafe;
    const all = [f.ours.content, f.base.content, f.theirs.content, f.workingTree];
    return !this.isBinary(f) && all.every((c) => c === null || !c.includes('\uFFFD'));
  }

  /** `editor` = tres paneles + resultado editable; `sides` = elegir una versión entera. */
  mode(f: ConflictFile): FileMode {
    if (this.isBinary(f) || !this.isTextSafe(f) || f.workingTree === null) return 'sides';
    return f.hunks.length > 0 || hasMarkers(f.workingTree) ? 'editor' : 'sides';
  }

  // ---- Sincronización ------------------------------------------------------------------

  /** Recarga el estado de la integración (mensaje por defecto, resumen). Idempotente. */
  async sync(): Promise<void> {
    if (!this.active) {
      this.reset();
      return;
    }
    this.reconcile();
    const seq = ++this.syncSeq;
    try {
      const st = await this.tab.api.mergeState(this.tab.id);
      if (seq !== this.syncSeq) return;
      this.state = st;
      if (!this.messageTouched) this.message = st.message;
    } catch {
      /* el estado se vuelve a pedir con el siguiente snapshot */
    }
  }

  private reset(): void {
    this.state = null;
    this.selectedKey = null;
    this.drafts = new Map();
    this.resolved = [];
    this.message = '';
    this.messageTouched = false;
    this.currentHunk = 0;
    this.abortWarning = false;
    this.aiProposal = null;
    this.aiError = null;
    this.aiLoading = false;
    this.aiRegenerating = false;
    this.aiRun++;
  }

  /** Mantiene una selección válida y descarta borradores de archivos que ya no están. */
  private reconcile(): void {
    const keys = new Set(this.files.map((f) => pathKey(f.path)));
    if (this.selectedKey === null || !keys.has(this.selectedKey)) {
      const first = this.files[0];
      this.selectedKey = first ? pathKey(first.path) : null;
      this.currentHunk = 0;
    }
    if ([...this.drafts.keys()].some((k) => !keys.has(k))) {
      this.drafts = new Map([...this.drafts].filter(([k]) => keys.has(k)));
    }
    // Una propuesta de un archivo que ya no está en conflicto no se puede usar.
    if (this.aiProposal && !keys.has(pathKey(this.aiProposal.path))) {
      this.aiProposal = null;
      this.aiError = null;
    }
  }

  select(f: ConflictFile): void {
    this.selectedKey = pathKey(f.path);
    this.currentHunk = 0;
    this.revealTick++;
  }

  /** Mueve la selección en la lista (±1) con envoltura. */
  step(delta: number): void {
    const list = this.files;
    if (list.length === 0) return;
    const i = Math.max(0, list.findIndex((f) => pathKey(f.path) === this.selectedKey));
    const next = list[(i + delta + list.length) % list.length];
    if (next) this.select(next);
  }

  setMessage(v: string): void {
    this.message = v;
    this.messageTouched = true;
  }

  // ---- Texto del resultado -------------------------------------------------------------

  text(f: ConflictFile): string {
    return this.drafts.get(pathKey(f.path))?.text ?? f.workingTree ?? '';
  }

  setText(f: ConflictFile, text: string): void {
    const key = pathKey(f.path);
    const next = new Map(this.drafts);
    if (text === (f.workingTree ?? '')) next.delete(key);
    else next.set(key, { text, original: f.workingTree });
    this.drafts = next;
    this.clampHunk(f);
  }

  isDirty(f: ConflictFile): boolean {
    return this.drafts.has(pathKey(f.path));
  }

  get dirtyCount(): number {
    return this.drafts.size;
  }

  /** El archivo cambió en disco mientras había una edición sin aplicar. */
  isStale(f: ConflictFile): boolean {
    const d = this.drafts.get(pathKey(f.path));
    return d !== undefined && d.original !== f.workingTree;
  }

  /** Descarta la edición y vuelve al contenido de disco. */
  discardDraft(f: ConflictFile): void {
    const next = new Map(this.drafts);
    next.delete(pathKey(f.path));
    this.drafts = next;
    this.currentHunk = 0;
  }

  /** «Conservar mi edición» tras un cambio externo: acepta el nuevo disco como punto de partida. */
  keepDraft(f: ConflictFile): void {
    const d = this.drafts.get(pathKey(f.path));
    if (!d) return;
    this.drafts = new Map(this.drafts).set(pathKey(f.path), { ...d, original: f.workingTree });
  }

  regions(f: ConflictFile) {
    return parseMarkers(this.text(f));
  }

  /** Bloques pendientes del resultado (incluye un `<<<<<<<` sin cerrar). */
  pending(f: ConflictFile): number {
    const p = this.regions(f);
    return p.regions.length + (p.unterminated ? 1 : 0);
  }

  canMarkResolved(f: ConflictFile): boolean {
    return this.mode(f) === 'editor' && this.busy === null && !hasMarkers(this.text(f));
  }

  private clampHunk(f: ConflictFile): void {
    const n = this.regions(f).regions.length;
    this.currentHunk = Math.max(0, Math.min(this.currentHunk, n - 1));
  }

  // ---- Bloques -------------------------------------------------------------------------

  goto(f: ConflictFile, index: number): void {
    const n = this.regions(f).regions.length;
    if (n === 0) return;
    this.currentHunk = ((index % n) + n) % n;
    this.revealTick++;
  }

  nextHunk(f: ConflictFile): void {
    this.goto(f, this.currentHunk + 1);
  }

  prevHunk(f: ConflictFile): void {
    this.goto(f, this.currentHunk - 1);
  }

  /** Resuelve el bloque actual con un lado. Devuelve `false` si no se pudo (p. ej. sin base). */
  resolveHunk(f: ConflictFile, side: HunkChoiceSide): boolean {
    const out = resolveRegion(this.text(f), this.currentHunk, side);
    if (out === null) return false;
    this.setText(f, out);
    if (this.pending(f) > 0) this.revealTick++;
    return true;
  }

  /** Aplica un lado a TODOS los bloques que queden (solo al texto; no escribe en disco). */
  resolveEverything(f: ConflictFile, side: HunkChoiceSide): void {
    this.setText(f, resolveAllRegions(this.text(f), side));
  }

  // ---- Resolución con IA ---------------------------------------------------------------

  get aiBusy(): boolean {
    return this.aiLoading || this.aiRegenerating;
  }

  /** La IA de la pestaña está activada (null = aún no se sabe; el botón funciona igual). */
  get aiEnabled(): boolean | null {
    const st = this.tab.ai.state;
    return st === null ? null : st.enabled;
  }

  /**
   * Pide a la IA el archivo entero resuelto. Solo propone: el editor no cambia hasta «Usar».
   * Rust lee las tres versiones del índice y aplica los filtros (sensibles, binarios, tope).
   */
  async resolveWithAi(f: ConflictFile): Promise<boolean> {
    if (this.aiBusy || this.busy !== null) return false;
    const mine = ++this.aiRun;
    this.aiLoading = true;
    this.aiError = null;
    this.aiProposal = null;
    toasts.announce(t('conflicts.ai.generating'));
    try {
      const s = await this.tab.api.aiResolveConflict(this.tab.id, f.path);
      if (mine !== this.aiRun) return false;
      this.aiProposal = s;
      return true;
    } catch (e) {
      if (mine !== this.aiRun) return false;
      const err = toApiError(e);
      if (err.code !== 'CANCELLED') this.aiError = err;
      return false;
    } finally {
      if (mine === this.aiRun) this.aiLoading = false;
    }
  }

  /** Cancela de verdad la generación en Rust (mata el proceso hijo). */
  async cancelAi(): Promise<void> {
    if (!this.aiBusy) return;
    this.aiRun++; // lo que llegue después se ignora
    this.aiLoading = false;
    this.aiRegenerating = false;
    toasts.announce(t('conflicts.ai.cancelled'));
    try {
      await this.tab.api.aiCancel(this.tab.id);
    } catch {
      /* ya había terminado */
    }
  }

  /** «Regenerar»: pide otra propuesta sin cerrar el panel. La anterior sigue visible. */
  async regenerateAi(f: ConflictFile): Promise<boolean> {
    if (this.aiBusy) return false;
    const mine = ++this.aiRun;
    this.aiRegenerating = true;
    this.aiError = null;
    try {
      const s = await this.tab.api.aiResolveConflict(this.tab.id, f.path);
      if (mine !== this.aiRun) return false;
      this.aiProposal = s;
      return true;
    } catch (e) {
      if (mine !== this.aiRun) return false;
      const err = toApiError(e);
      if (err.code !== 'CANCELLED') this.aiError = err;
      return false;
    } finally {
      if (mine === this.aiRun) this.aiRegenerating = false;
    }
  }

  /**
   * «Usar»: pone la propuesta en el editor como texto editable (un borrador). NO escribe en
   * disco ni marca nada: los contadores de bloques se recalculan solos sobre el texto nuevo.
   */
  useAiProposal(f: ConflictFile): boolean {
    const p = this.aiProposal;
    if (!p) return false;
    this.aiProposal = null;
    this.aiError = null;
    this.setText(f, withEol(p.content, dominantEol(f.workingTree ?? '')));
    this.revealTick++;
    toasts.announce(t('conflicts.ai.used'));
    return true;
  }

  /** «Descartar»: tira la propuesta y deja el editor como estaba. */
  discardAiProposal(): void {
    if (!this.aiProposal && !this.aiError) return;
    this.aiProposal = null;
    this.aiError = null;
    toasts.announce(t('conflicts.ai.discarded'));
  }

  dismissAiError(): void {
    this.aiError = null;
  }

  // ---- Acciones contra Rust ------------------------------------------------------------

  private async run<T>(label: string, title: string, fn: () => Promise<T>): Promise<T | undefined> {
    if (this.busy) return undefined;
    this.busy = label;
    try {
      return await fn();
    } catch (e) {
      toasts.error(title, toApiError(e));
      return undefined;
    } finally {
      this.busy = null;
    }
  }

  private markDone(f: ConflictFile): void {
    const key = pathKey(f.path);
    const next = new Map(this.drafts);
    next.delete(key);
    this.drafts = next;
    this.resolved = [...this.resolved, pathText(f.path)];
    toasts.success(t('conflicts.resolvedToast', { path: pathText(f.path) }));
    toasts.announce(t('conflicts.resolvedToast', { path: pathText(f.path) }));
  }

  /** «Marcar como resuelto»: escribe el texto del resultado y hace `git add` (Rust confirma). */
  async markResolved(f: ConflictFile): Promise<boolean> {
    if (!this.canMarkResolved(f)) return false;
    const bytes = Array.from(encoder.encode(this.text(f)));
    const res = await this.run('resolve', t('conflicts.resolveFailed'), () =>
      this.tab.api.resolveConflict(this.tab.id, f.path, { manual: bytes })
    );
    if (!res?.done) return false;
    this.markDone(f);
    return true;
  }

  /** Versión entera del índice: ours / theirs (/ base). Si ese lado no existe, borra el archivo. */
  async acceptSide(f: ConflictFile, side: ConflictChoice): Promise<boolean> {
    const res = await this.run('resolve', t('conflicts.resolveFailed'), () =>
      this.tab.api.resolveConflict(this.tab.id, f.path, { side })
    );
    if (!res?.done) return false;
    this.markDone(f);
    return true;
  }

  /** Termina el merge (o continúa rebase/cherry-pick). */
  async continueIntegration(): Promise<boolean> {
    if (!this.canContinue) return false;
    const merging = this.integration === 'merging';
    const message = merging && this.message.trim() !== '' ? this.message : null;
    const res = await this.run('continue', t('conflicts.continueFailed'), () => this.tab.api.mergeContinue(this.tab.id, message));
    if (!res?.done) return false;
    if (res.integration === 'none') {
      toasts.success(t('conflicts.continued'), res.sha ? res.sha.slice(0, 7) : undefined);
      toasts.announce(t('conflicts.continued'));
      this.tab.panel = 'changes';
    } else {
      toasts.warning(t('conflicts.nextStep'), tn('conflicts.nextStepBody', this.files.length));
    }
    return true;
  }

  /** Pide abortar. Con ediciones sin aplicar avisa primero (`abortWarning`); Rust confirma después. */
  async abort(confirmedLoss = false): Promise<boolean> {
    if (this.dirtyCount > 0 && !confirmedLoss) {
      this.abortWarning = true;
      return false;
    }
    this.abortWarning = false;
    const merging = this.integration === 'merging';
    const res = await this.run('abort', t('conflicts.abortFailed'), () =>
      merging ? this.tab.api.mergeAbort(this.tab.id) : this.tab.api.abortIntegration(this.tab.id)
    );
    if (!res?.done) return false;
    toasts.success(t('conflicts.aborted'));
    this.tab.panel = 'changes';
    return true;
  }

  cancelAbortWarning(): void {
    this.abortWarning = false;
  }

  /** Aviso al salir del panel con ediciones sin aplicar. */
  get unsavedWarning(): string | null {
    return this.dirtyCount > 0 ? tn('conflicts.unsaved', this.dirtyCount) : null;
  }
}

const registry = new WeakMap<TabState, ConflictsController>();

export function conflictsOf(tab: TabState): ConflictsController {
  let c = registry.get(tab);
  if (!c) {
    c = new ConflictsController(tab);
    registry.set(tab, c);
  }
  return c;
}
