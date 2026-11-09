import { es, type MessageKey } from './es';

export type { MessageKey };

/** Catálogos disponibles. Para añadir inglés: `en.ts` con las mismas claves y registrarlo aquí. */
const catalogs: Record<string, Record<MessageKey, string>> = { es };
let current = 'es';

export function setLocale(locale: string): void {
  if (catalogs[locale]) current = locale;
}

export function getLocale(): string {
  return current;
}

export function registerCatalog(locale: string, catalog: Record<MessageKey, string>): void {
  catalogs[locale] = catalog;
}

type Params = Record<string, string | number>;

function format(template: string, params?: Params): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (m, k: string) => (k in params ? String(params[k]) : m));
}

/** Traduce una clave con interpolación `{nombre}`. */
export function t(key: MessageKey, params?: Params): string {
  const cat = catalogs[current] ?? es;
  return format(cat[key] ?? es[key] ?? key, params);
}

/** Plural simple: busca `clave.one` / `clave.other` (el catálogo debe tener ambas). */
export function tn(key: string, n: number, params?: Params): string {
  const suffix = n === 1 ? 'one' : 'other';
  return t(`${key}.${suffix}` as MessageKey, { n, ...params });
}
