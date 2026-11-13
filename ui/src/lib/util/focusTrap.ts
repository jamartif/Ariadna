const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusables(node: HTMLElement): HTMLElement[] {
  return Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => !el.hasAttribute('hidden') && el.getAttribute('aria-hidden') !== 'true'
  );
}

/**
 * Acción Svelte: trampa de foco para diálogos. Enfoca el primer elemento (o el
 * marcado con `data-autofocus`), cicla Tab/Shift+Tab y devuelve el foco al
 * elemento que lo tenía al abrir.
 */
export function focusTrap(node: HTMLElement) {
  const previous = document.activeElement as HTMLElement | null;
  const initial = node.querySelector<HTMLElement>('[data-autofocus]') ?? focusables(node)[0] ?? node;
  if (initial === node && !node.hasAttribute('tabindex')) node.setAttribute('tabindex', '-1');
  initial.focus();

  function onKeydown(e: KeyboardEvent) {
    if (e.key !== 'Tab') return;
    const items = focusables(node);
    if (items.length === 0) {
      e.preventDefault();
      return;
    }
    const first = items[0] as HTMLElement;
    const last = items[items.length - 1] as HTMLElement;
    const active = document.activeElement;
    if (e.shiftKey && (active === first || !node.contains(active))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (active === last || !node.contains(active))) {
      e.preventDefault();
      first.focus();
    }
  }
  node.addEventListener('keydown', onKeydown);
  return {
    destroy() {
      node.removeEventListener('keydown', onKeydown);
      if (previous && document.contains(previous)) previous.focus();
    }
  };
}
