import type { Api } from '$lib/api/api';
import { toApiError, type ApiError, type CloneProgress, type OpenResult } from '$lib/api/types';
import { checkCloneUrl, joinPath, parseRemote, suggestFolderName, type CloneUrlProblem, type RemoteGuess } from '$lib/util/remote';

export type CloneStatus = 'idle' | 'cloning' | 'error';

/**
 * Lógica del diálogo «Clonar repositorio».
 *
 * SEGURIDAD: el PAT (`secret`) es de **un solo uso**. Vive solo aquí, en memoria, mientras se
 * escribe; al pulsar «Clonar» se entrega a Rust (`clone_repository`, que se lo pasa a git por
 * `GIT_ASKPASS` temporal) y se borra al instante, haya éxito o error. No se copia a ningún otro
 * store ni al borrador, no se guarda, no se registra y nunca forma parte de un mensaje de error.
 */
export class CloneFlow {
  url = $state('');
  destination = $state('');
  /** Si el usuario editó la carpeta a mano no se vuelve a sugerir. */
  private destEdited = false;
  private parent = '';
  branch = $state('');

  username = $state('');
  secret = $state('');
  showSecret = $state(false);
  credOpen = $state(false);

  status = $state<CloneStatus>('idle');
  error = $state.raw<ApiError | null>(null);
  progress = $state.raw<CloneProgress | null>(null);

  private unlisten: (() => void) | null = null;
  private operationId: number | null = null;
  private cancelled = false;

  constructor(
    readonly api: Api,
    private readonly onopened: (res: OpenResult) => void
  ) {}

  // ---- Derivados -------------------------------------------------------------------------

  get guess(): RemoteGuess | null {
    return parseRemote(this.url);
  }

  get problem(): CloneUrlProblem | null {
    return checkCloneUrl(this.url);
  }

  /** Las credenciales solo aplican a HTTPS: SSH usa la clave del usuario. */
  get wantsCredentials(): boolean {
    return this.guess?.scheme === 'https';
  }

  get canClone(): boolean {
    return this.status !== 'cloning' && this.problem === null && this.destination.trim() !== '';
  }

  // ---- Edición ---------------------------------------------------------------------------

  setUrl(v: string): void {
    this.url = v;
    this.error = null;
    this.resuggest();
  }

  setParent(path: string): void {
    this.parent = path;
    this.destEdited = false;
    this.resuggest();
  }

  setDestination(v: string): void {
    this.destination = v;
    this.destEdited = true;
  }

  private resuggest(): void {
    if (this.destEdited) return;
    const name = suggestFolderName(this.url);
    if (this.parent !== '' && name !== '') this.destination = joinPath(this.parent, name);
    else if (this.parent !== '') this.destination = this.parent;
  }

  openCredentials(): void {
    this.credOpen = true;
  }

  // ---- Clonado -----------------------------------------------------------------------------

  async clone(): Promise<boolean> {
    if (!this.canClone) return false;
    this.status = 'cloning';
    this.error = null;
    this.progress = { operationId: 0, phase: '', percent: null };
    this.operationId = null;
    this.cancelled = false;
    // El token sale de la interfaz en este mismo instante: se pasa a Rust una sola vez.
    const auth = this.wantsCredentials && this.secret !== '' ? { username: this.username.trim(), secret: this.secret } : null;
    this.secret = '';
    this.showSecret = false;
    try {
      this.unlisten = await this.api.listen('clone-progress', (p) => {
        if (this.operationId === null) this.operationId = p.operationId;
        if (p.operationId === this.operationId) this.progress = p;
      });
    } catch {
      /* sin progreso: sigue funcionando */
    }
    try {
      const res = await this.api.cloneRepository(this.url.trim(), this.destination.trim(), this.branch.trim() === '' ? null : this.branch.trim(), auth);
      this.status = 'idle';
      this.onopened(res);
      return true;
    } catch (e) {
      const err = toApiError(e);
      if (err.code === 'CANCELLED' || this.cancelled) {
        this.status = 'idle';
        this.error = null;
      } else {
        this.status = 'error';
        this.error = err;
        // Un error de autenticación abre el formulario de credenciales.
        if (err.code === 'AUTH' && this.wantsCredentials) this.credOpen = true;
      }
      return false;
    } finally {
      this.unlisten?.();
      this.unlisten = null;
      this.progress = null;
      this.operationId = null;
    }
  }

  async cancel(): Promise<void> {
    if (this.status !== 'cloning') return;
    this.cancelled = true;
    if (this.operationId !== null) {
      try {
        await this.api.cancelOperation('', this.operationId);
      } catch {
        /* ya terminó */
      }
    }
  }

  dispose(): void {
    this.unlisten?.();
    this.unlisten = null;
    this.secret = '';
  }
}
