import type { Api } from '$lib/api/api';
import {
  ApiError,
  toApiError,
  type ApiError as ApiErrorType,
  type AuthMethodId,
  type AuthOptions,
  type CliAuthStatus,
  type CloneAuthRequest,
  type CloneProgress,
  type DeviceLoginStart,
  type OpenResult
} from '$lib/api/types';
import {
  checkCloneUrl,
  joinPath,
  parseRemote,
  suggestFolderName,
  type CloneUrlProblem,
  type RemoteGuess
} from '$lib/util/remote';

export type CloneStatus = 'idle' | 'cloning' | 'error';
/** Resultado del botón «Comprobar» (`ls-remote`, no clona). */
export type CheckState = 'idle' | 'checking' | 'ok' | 'failed';
/** Estado del flujo OAuth device code. */
export type OAuthState = 'idle' | 'starting' | 'waiting' | 'authorized' | 'expired' | 'denied' | 'error';

/** Mínimo entre sondeos del flujo OAuth (el servidor suele pedir 5 s; en pruebas, menos). */
const MIN_POLL_MS = 300;

/**
 * Lógica del diálogo «Clonar repositorio».
 *
 * SEGURIDAD: solo el PAT (`secret`) es texto que escribe el usuario. Es de **un solo uso**: vive
 * aquí, en memoria, mientras se escribe; al pulsar «Clonar» se entrega a Rust (que lo pasa a git
 * por `GIT_ASKPASS` temporal) y se borra al instante, haya éxito o error. No se copia a ningún
 * otro store ni al borrador, no se guarda, no se registra y nunca forma parte de un mensaje de
 * error.
 *
 * Con «Sesión de GitHub/GitLab CLI» **no hay token en la interfaz**: se manda el método y Rust
 * pide el token al CLI. Con OAuth, la interfaz solo maneja un `flowId` opaco: el token se queda
 * en memoria de Rust hasta el clonado.
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

  /** Método elegido: PAT (por defecto), sesión de `gh`/`glab` u OAuth. */
  method = $state<AuthMethodId>('pat');
  /** Estado real de los CLI y del OAuth para esta URL (lo comprueba Rust). */
  authOptions = $state.raw<AuthOptions | null>(null);
  authLoading = $state(false);
  private authForUrl = '';

  checkState = $state<CheckState>('idle');
  checkError = $state.raw<ApiErrorType | null>(null);

  oauth = $state.raw<DeviceLoginStart | null>(null);
  oauthState = $state<OAuthState>('idle');
  oauthError = $state.raw<ApiErrorType | null>(null);

  status = $state<CloneStatus>('idle');
  error = $state.raw<ApiErrorType | null>(null);
  progress = $state.raw<CloneProgress | null>(null);

  private unlisten: (() => void) | null = null;
  private operationId: number | null = null;
  private cancelled = false;
  private pollTimer: ReturnType<typeof setTimeout> | null = null;
  private disposed = false;

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

  /** Sesión del CLI que corresponde al método elegido (`null` si el método no es de CLI). */
  get cliStatus(): CliAuthStatus | null {
    return this.authOptions?.cli.find((c) => c.id === this.method) ?? null;
  }

  /** Hay flujo OAuth configurado (el usuario puso su `client_id` en Ajustes). */
  get oauthAvailable(): boolean {
    return this.authOptions?.oauth?.available === true;
  }

  /** Métodos que la interfaz puede ofrecer para esta URL. */
  get methods(): AuthMethodId[] {
    const list: AuthMethodId[] = ['pat'];
    for (const c of this.authOptions?.cli ?? []) {
      if (c.id === 'gh' || c.id === 'glab') list.push(c.id);
    }
    if (this.oauthAvailable) list.push('oauth');
    return list;
  }

  get canClone(): boolean {
    if (this.status === 'cloning' || this.problem !== null || this.destination.trim() === '') return false;
    if (this.method === 'gh' || this.method === 'glab') {
      const cli = this.cliStatus;
      return !!cli?.installed && !!cli?.loggedIn;
    }
    if (this.method === 'oauth') return this.oauthState === 'authorized';
    return true;
  }

  /** `true` si el botón «Comprobar» tiene sentido (URL válida y sin clonar). */
  get canCheck(): boolean {
    return this.status !== 'cloning' && this.problem === null && this.checkState !== 'checking';
  }

  // ---- Edición ---------------------------------------------------------------------------

  setUrl(v: string): void {
    const before = this.guess?.provider ?? null;
    this.url = v;
    this.error = null;
    this.checkState = 'idle';
    this.checkError = null;
    this.resuggest();
    if (this.guess?.provider !== before) {
      // Cambió el proveedor: otra vez el estado de los CLI (y el flujo OAuth deja de valer).
      void this.stopOauth();
      void this.loadAuthOptions();
    }
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

  // ---- Métodos de autenticación ------------------------------------------------------------

  /** Pide a Rust el estado real de `gh`/`glab` y del OAuth para la URL actual. */
  async loadAuthOptions(): Promise<void> {
    if (!this.wantsCredentials) {
      this.authOptions = null;
      this.authForUrl = '';
      this.method = 'pat';
      return;
    }
    const url = this.url.trim();
    if (url === this.authForUrl) return;
    this.authForUrl = url;
    this.authLoading = true;
    try {
      const options = await this.api.cloneAuthOptions(url);
      if (this.disposed) return;
      this.authOptions = options;
      // Si el método elegido ya no existe para esta URL, se vuelve al PAT.
      if (!this.methods.includes(this.method)) {
        this.method = 'pat';
        void this.stopOauth();
      }
    } catch {
      // Sin estado de CLI el diálogo sigue funcionando con el PAT.
      this.authOptions = null;
    } finally {
      this.authLoading = false;
    }
  }

  setMethod(m: AuthMethodId): void {
    if (this.method === m) return;
    this.method = m;
    this.checkState = 'idle';
    this.checkError = null;
    if (m !== 'oauth') void this.stopOauth();
  }

  /** Credencial a enviar, o `null` si no hay ninguna (repo público). */
  private authRequest(): CloneAuthRequest | null {
    if (!this.wantsCredentials) return null;
    if (this.method === 'gh' || this.method === 'glab') return { method: this.method };
    if (this.method === 'oauth') {
      return this.oauth && this.oauthState === 'authorized' ? { method: 'oauth', flowId: this.oauth.flowId } : null;
    }
    const secret = this.secret === '' ? null : this.secret;
    const username = this.username.trim();
    return secret === null && username === '' ? null : { method: 'pat', username, secret };
  }

  /** `git ls-remote` con el método elegido: valida el acceso sin descargar nada. */
  async check(): Promise<void> {
    if (!this.canCheck) return;
    this.checkState = 'checking';
    this.checkError = null;
    try {
      const res = await this.api.testCredential(this.url.trim(), this.authRequest());
      if (this.disposed) return;
      if (res.ok) {
        this.checkState = 'ok';
      } else {
        this.checkState = 'failed';
        this.checkError = new ApiError({ code: 'AUTH', message: '', detail: res.code });
      }
    } catch (e) {
      if (this.disposed) return;
      this.checkState = 'failed';
      this.checkError = toApiError(e);
    }
  }

  // ---- OAuth device code -------------------------------------------------------------------

  /** Paso 1: pide el código, abre la página de verificación y empieza a sondear. */
  async startOauth(): Promise<void> {
    if (this.oauthState === 'starting' || this.oauthState === 'waiting') return;
    this.oauthState = 'starting';
    this.oauthError = null;
    try {
      const start = await this.api.oauthDeviceStart(this.url.trim());
      if (this.disposed) return;
      this.oauth = start;
      this.oauthState = 'waiting';
      try {
        await this.api.openExternalUrl(start.verificationUri);
      } catch {
        /* sin navegador, la página se abre a mano con el enlace que se muestra */
      }
      this.schedulePoll(start.interval * 1000);
    } catch (e) {
      if (this.disposed) return;
      this.oauth = null;
      this.oauthState = 'error';
      this.oauthError = toApiError(e);
    }
  }

  /** Abandona el flujo (y su token) y vuelve a «sin iniciar». */
  async stopOauth(): Promise<void> {
    this.clearTimer();
    const flow = this.oauth;
    this.oauth = null;
    this.oauthError = null;
    if (this.oauthState === 'waiting' || this.oauthState === 'starting' || this.oauthState === 'authorized') {
      this.oauthState = 'idle';
    }
    if (flow) {
      try {
        await this.api.oauthDeviceCancel(flow.flowId);
      } catch {
        /* ya no está en curso */
      }
    }
  }

  private schedulePoll(ms: number): void {
    this.clearTimer();
    const wait = Math.min(Math.max(MIN_POLL_MS, ms), 60_000);
    this.pollTimer = setTimeout(() => {
      this.pollTimer = null;
      void this.poll();
    }, wait);
  }

  private clearTimer(): void {
    if (this.pollTimer !== null) {
      clearTimeout(this.pollTimer);
      this.pollTimer = null;
    }
  }

  /** Paso 3: un sondeo. `slow_down` alarga la espera; `authorized` habilita el clonado. */
  private async poll(): Promise<void> {
    const flow = this.oauth;
    if (!flow || this.disposed || this.oauthState !== 'waiting') return;
    try {
      const res = await this.api.oauthDevicePoll(flow.flowId);
      if (this.disposed || this.oauthState !== 'waiting') return;
      switch (res.state) {
        case 'authorized':
          this.oauthState = 'authorized';
          this.checkState = 'idle';
          break;
        case 'expired':
          this.oauth = null;
          this.oauthState = 'expired';
          break;
        case 'denied':
          this.oauth = null;
          this.oauthState = 'denied';
          break;
        default:
          this.schedulePoll((res.interval ?? flow.interval) * 1000);
          break;
      }
    } catch (e) {
      if (this.disposed) return;
      this.oauthState = 'error';
      this.oauthError = toApiError(e);
    }
  }

  // ---- Clonado -----------------------------------------------------------------------------

  async clone(): Promise<boolean> {
    if (!this.canClone) return false;
    this.status = 'cloning';
    this.error = null;
    this.progress = { operationId: 0, phase: '', percent: null };
    this.operationId = null;
    this.cancelled = false;
    // El PAT sale de la interfaz en este mismo instante: se pasa a Rust una sola vez.
    const auth = this.authRequest();
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
      const res = await this.api.cloneRepository(
        this.url.trim(),
        this.destination.trim(),
        this.branch.trim() === '' ? null : this.branch.trim(),
        auth
      );
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
    this.disposed = true;
    this.clearTimer();
    this.unlisten?.();
    this.unlisten = null;
    this.secret = '';
    void this.stopOauth();
  }
}
