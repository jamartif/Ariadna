import { en } from './en';
import { es, type MessageKey } from './es';
import { localeState, type Locale } from './locale.svelte';

export type { Locale, MessageKey };

/** Preferencia de idioma guardada: un idioma concreto o «seguir al sistema». */
export type LanguagePref = Locale | 'system';

export const LOCALES: readonly Locale[] = ['es', 'en'];
export const DEFAULT_LOCALE: Locale = 'es';
/**
 * Clave de `localStorage` con la preferencia de idioma. Es un espejo de `settings.json` para
 * pintar el idioma correcto en el primer render; la fuente de verdad es el ajuste de Rust.
 */
export const LOCALE_KEY = 'ariadna.locale.v1';

/** Catálogos disponibles: cada idioma debe tener todas las claves de `MessageKey`. */
const catalogs: Record<Locale, Record<MessageKey, string>> = { es, en };

const hasDom = (): boolean => typeof document !== 'undefined';

export function isLocale(v: unknown): v is Locale {
  return v === 'es' || v === 'en';
}

export function isLanguagePref(v: unknown): v is LanguagePref {
  return v === 'system' || isLocale(v);
}

/** `es`/`es-*` → español; cualquier otro idioma → inglés; sin información → español. */
export function localeFromLanguage(tag: string | null | undefined): Locale {
  const l = (tag ?? '').trim().toLowerCase();
  if (l === '') return DEFAULT_LOCALE;
  return l === 'es' || l.startsWith('es-') ? 'es' : 'en';
}

/** Idioma del sistema (`navigator.languages`, con `navigator.language` de reserva). */
export function systemLocale(): Locale {
  if (typeof navigator === 'undefined') return DEFAULT_LOCALE;
  const tags = navigator.languages?.length ? navigator.languages : [navigator.language];
  return localeFromLanguage(tags[0]);
}

/** Preferencia guardada en este equipo, o `system` si no hay ninguna. */
export function storedPref(): LanguagePref {
  try {
    const raw = localStorage.getItem(LOCALE_KEY);
    if (isLanguagePref(raw)) return raw;
  } catch {
    /* almacenamiento no disponible */
  }
  return 'system';
}

/** Preferencia → idioma real (`system` se resuelve con el idioma del sistema). */
export function resolveLocale(pref: string): Locale {
  return isLocale(pref) ? pref : systemLocale();
}

/** Idioma con el que debe arrancar la ventana: lo guardado o, si no hay nada, el del sistema. */
export function detectLocale(): Locale {
  return resolveLocale(storedPref());
}

/** Cambia el idioma activo (solo si existe el catálogo) y actualiza `<html lang>`. */
export function setLocale(locale: string): void {
  localeState.locale = isLocale(locale) ? locale : DEFAULT_LOCALE;
  if (hasDom()) document.documentElement.lang = localeState.locale;
}

export function getLocale(): Locale {
  return localeState.locale;
}

/**
 * Etiqueta BCP 47 del idioma activo para `toLocaleString`/`Intl` (fechas y números).
 * Se lee en cada llamada, así que un cambio de idioma también reformatea las fechas.
 */
export function intlLocale(): string {
  return localeState.locale === 'es' ? 'es-ES' : 'en-US';
}

/** Registra un catálogo extra (pruebas y ampliaciones); debe tener todas las claves. */
export function registerCatalog(locale: string, catalog: Record<MessageKey, string>): void {
  if (isLocale(locale)) catalogs[locale] = catalog;
}

/**
 * Aplica una preferencia: la guarda en `localStorage` (espejo para el arranque) y cambia el
 * idioma al momento. Devuelve el idioma real aplicado.
 */
export function applyLanguage(pref: LanguagePref): Locale {
  try {
    localStorage.setItem(LOCALE_KEY, pref);
  } catch {
    /* almacenamiento no disponible */
  }
  setLocale(resolveLocale(pref));
  return localeState.locale;
}

/** Primer paso del arranque: aplica el idioma guardado antes de montar la interfaz. */
export function initLocale(): Locale {
  return applyLanguage(storedPref());
}

type Params = Record<string, string | number>;

function format(template: string, params?: Params): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (m, k: string) => (k in params ? String(params[k]) : m));
}

/** Traduce una clave con interpolación `{nombre}`. */
export function t(key: MessageKey, params?: Params): string {
  const cat = catalogs[localeState.locale] ?? es;
  return format(cat[key] ?? es[key] ?? key, params);
}

/** Plural simple: busca `clave.one` / `clave.other` (el catálogo debe tener ambas). */
export function tn(key: string, n: number, params?: Params): string {
  const suffix = n === 1 ? 'one' : 'other';
  return t(`${key}.${suffix}` as MessageKey, { n, ...params });
}
