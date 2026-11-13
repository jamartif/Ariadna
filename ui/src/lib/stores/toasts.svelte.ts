import { ApiError } from '$lib/api/types';

export type ToastKind = 'error' | 'success' | 'info' | 'warning';

export interface Toast {
  id: number;
  kind: ToastKind;
  title: string;
  message?: string;
  detail?: string | null;
}

class ToastStore {
  items = $state.raw<Toast[]>([]);
  /** Texto para la región `aria-live` (operaciones). */
  announcement = $state('');
  private nextId = 1;
  private timers = new Map<number, ReturnType<typeof setTimeout>>();

  push(t: Omit<Toast, 'id'>, ttl?: number): number {
    const id = this.nextId++;
    this.items = [...this.items, { ...t, id }].slice(-5);
    const life = ttl ?? (t.kind === 'error' ? 0 : t.kind === 'warning' ? 8000 : 5000);
    if (life > 0) this.timers.set(id, setTimeout(() => this.dismiss(id), life));
    return id;
  }

  error(title: string, e: unknown): number {
    const message = e instanceof ApiError ? e.message : e instanceof Error ? e.message : String(e);
    const detail = e instanceof ApiError ? e.detail : null;
    return this.push({ kind: 'error', title, message, detail });
  }

  success(title: string, message?: string): number {
    return this.push({ kind: 'success', title, message });
  }

  info(title: string, message?: string): number {
    return this.push({ kind: 'info', title, message });
  }

  warning(title: string, message?: string): number {
    return this.push({ kind: 'warning', title, message });
  }

  dismiss(id: number): void {
    const timer = this.timers.get(id);
    if (timer) clearTimeout(timer);
    this.timers.delete(id);
    this.items = this.items.filter((t) => t.id !== id);
  }

  clear(): void {
    for (const id of [...this.timers.keys()]) this.dismiss(id);
    this.items = [];
    this.announcement = '';
  }

  announce(text: string): void {
    // Alterna un espacio final para que lectores de pantalla repitan mensajes idénticos.
    this.announcement = this.announcement === text ? `${text}\u00a0` : text;
  }
}

export const toasts = new ToastStore();
