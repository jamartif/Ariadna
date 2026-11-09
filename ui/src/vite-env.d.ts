/// <reference types="svelte" />
/// <reference types="vite/client" />

interface Window {
  __TAURI_INTERNALS__?: unknown;
}

// Gramáticas de Prism (se cargan bajo demanda; el paquete no tipa cada componente).
declare module 'prismjs/components/*.js';
