import '@testing-library/jest-dom/vitest';

// jsdom no implementa ResizeObserver (lo usa `bind:clientHeight` de Svelte). Sin diseño real
// las listas virtualizadas usan su altura de reserva.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as unknown as typeof ResizeObserver;


// jsdom no implementa Element.animate (transiciones de Svelte y reordenado de pestañas):
// stub mínimo que termina al instante.
if (typeof Element !== 'undefined' && !Element.prototype.animate) {
  Element.prototype.animate = function () {
    const anim = {
      onfinish: null as null | (() => void),
      oncancel: null as null | (() => void),
      cancel() {},
      finish() {},
      play() {},
      pause() {},
      reverse() {},
      commitStyles() {},
      persist() {},
      currentTime: 0,
      playState: 'finished',
      finished: Promise.resolve(),
      addEventListener() {},
      removeEventListener() {}
    };
    queueMicrotask(() => anim.onfinish?.());
    return anim as unknown as Animation;
  };
}
if (typeof Element !== 'undefined' && !Element.prototype.getAnimations) {
  Element.prototype.getAnimations = () => [];
}
