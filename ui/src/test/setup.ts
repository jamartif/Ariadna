import '@testing-library/jest-dom/vitest';

// jsdom no implementa ResizeObserver (lo usa `bind:clientHeight` de Svelte). Sin diseño real
// las listas virtualizadas usan su altura de reserva.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as unknown as typeof ResizeObserver;
