/**
 * Controles de ventana propios (ipc.md, «Ventana»). `@tauri-apps/api/window` solo
 * funciona dentro de Tauri: en el navegador (mock) devuelve `null` y los controles
 * son inertes pero se ven, para poder diseñarlos y probarlos.
 */
export interface WindowApi {
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
  isMaximized(): Promise<boolean>;
  /** Avisa cuando cambia el tamaño (para el icono maximizar/restaurar). Devuelve el desuscriptor. */
  onResized(cb: () => void): Promise<() => void>;
}

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function getWindowApi(): Promise<WindowApi | null> {
  if (!isTauriRuntime()) return null;
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const w = getCurrentWindow();
    return {
      minimize: () => w.minimize(),
      toggleMaximize: () => w.toggleMaximize(),
      close: () => w.close(),
      isMaximized: () => w.isMaximized(),
      onResized: async (cb) => w.onResized(() => cb())
    };
  } catch {
    return null;
  }
}
