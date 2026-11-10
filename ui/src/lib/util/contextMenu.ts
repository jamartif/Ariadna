/**
 * Suprime el menú contextual genérico del navegador («Atrás», «Recargar», «Inspeccionar»…), que
 * no tiene sentido en una app de escritorio. Se conserva en los campos de texto editables
 * (copiar, pegar, corrector) y cuando hay texto seleccionado.
 */
export function shouldKeepNativeMenu(target: EventTarget | null, selection: string): boolean {
  if (selection.trim().length > 0) return true;
  if (!(target instanceof Element)) return false;
  const el = target.closest('input, textarea, [contenteditable=""], [contenteditable="true"]');
  return el !== null;
}

export function installContextMenuGuard(doc: Document = document): () => void {
  const handler = (e: MouseEvent) => {
    const sel = doc.defaultView?.getSelection()?.toString() ?? '';
    if (!shouldKeepNativeMenu(e.target, sel)) e.preventDefault();
  };
  doc.addEventListener('contextmenu', handler);
  return () => doc.removeEventListener('contextmenu', handler);
}
