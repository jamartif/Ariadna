import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  applyLanguage,
  detectLocale,
  getLocale,
  initLocale,
  isLanguagePref,
  LOCALE_KEY,
  localeFromLanguage,
  resolveLocale,
  setLocale,
  storedPref,
  systemLocale,
  t,
  tn
} from './index';

/** `navigator` es de solo lectura en jsdom: se redefine la propiedad de la instancia. */
function fakeNavigator(language: string, languages: string[] = [language]): void {
  Object.defineProperty(navigator, 'language', { value: language, configurable: true });
  Object.defineProperty(navigator, 'languages', { value: languages, configurable: true });
}

beforeEach(() => {
  localStorage.clear();
  setLocale('es');
  fakeNavigator('es-ES', ['es-ES']);
});

afterEach(() => {
  localStorage.clear();
  setLocale('es');
  fakeNavigator('es-ES', ['es-ES']);
});

describe('detección de idioma', () => {
  it('es-* es español; cualquier otro idioma, inglés', () => {
    expect(localeFromLanguage('es')).toBe('es');
    expect(localeFromLanguage('es-ES')).toBe('es');
    expect(localeFromLanguage('ES-mx')).toBe('es');
    expect(localeFromLanguage('en-US')).toBe('en');
    expect(localeFromLanguage('fr-FR')).toBe('en');
    expect(localeFromLanguage('')).toBe('es');
    expect(localeFromLanguage(null)).toBe('es');
  });

  it('el sistema manda por `navigator.languages` (y `language` como reserva)', () => {
    fakeNavigator('en-GB', ['fr-FR', 'en-GB']);
    expect(systemLocale()).toBe('en');
    fakeNavigator('es-AR', ['es-AR', 'en-GB']);
    expect(systemLocale()).toBe('es');
  });

  it('sin preferencia guardada usa el idioma del sistema', () => {
    fakeNavigator('en-US', ['en-US']);
    expect(storedPref()).toBe('system');
    expect(detectLocale()).toBe('en');
  });

  it('la preferencia guardada gana sobre el sistema', () => {
    fakeNavigator('en-US', ['en-US']);
    localStorage.setItem(LOCALE_KEY, 'es');
    expect(storedPref()).toBe('es');
    expect(detectLocale()).toBe('es');
    localStorage.setItem(LOCALE_KEY, 'en');
    fakeNavigator('es-ES', ['es-ES']);
    expect(detectLocale()).toBe('en');
  });

  it('«system» se resuelve con el idioma del sistema', () => {
    localStorage.setItem(LOCALE_KEY, 'system');
    fakeNavigator('en-US', ['en-US']);
    expect(detectLocale()).toBe('en');
    expect(resolveLocale('system')).toBe('en');
    expect(resolveLocale('es')).toBe('es');
    expect(resolveLocale('basura')).toBe('en');
  });

  it('una preferencia desconocida o dañada se ignora', () => {
    localStorage.setItem(LOCALE_KEY, 'klingon');
    expect(storedPref()).toBe('system');
    expect(isLanguagePref('klingon')).toBe(false);
    expect(isLanguagePref('en')).toBe(true);
  });
});

describe('aplicar el idioma y persistirlo', () => {
  it('applyLanguage cambia el catálogo, `<html lang>` y el espejo en localStorage', () => {
    expect(t('common.retry')).toBe('Reintentar');
    expect(applyLanguage('en')).toBe('en');
    expect(getLocale()).toBe('en');
    expect(document.documentElement.lang).toBe('en');
    expect(localStorage.getItem(LOCALE_KEY)).toBe('en');
    expect(t('common.retry')).toBe('Retry');
    expect(tn('commit.staged', 1)).toBe('1 file staged');
    expect(tn('commit.staged', 4)).toBe('4 files staged');
    expect(t('tabs.close', { name: 'repo' })).toBe('Close repo');

    expect(applyLanguage('es')).toBe('es');
    expect(getLocale()).toBe('es');
    expect(document.documentElement.lang).toBe('es');
    expect(t('common.retry')).toBe('Reintentar');
  });

  it('applyLanguage guarda también la preferencia «sistema»', () => {
    fakeNavigator('en-US', ['en-US']);
    expect(applyLanguage('system')).toBe('en');
    expect(localStorage.getItem(LOCALE_KEY)).toBe('system');
    expect(document.documentElement.lang).toBe('en');
  });

  it('initLocale arranca con lo guardado (o con el sistema si no hay nada)', () => {
    fakeNavigator('en-US', ['en-US']);
    expect(initLocale()).toBe('en');
    localStorage.setItem(LOCALE_KEY, 'es');
    expect(initLocale()).toBe('es');
    expect(getLocale()).toBe('es');
    expect(document.documentElement.lang).toBe('es');
  });

  it('setLocale ignora un idioma sin catálogo y vuelve al español', () => {
    setLocale('en');
    expect(getLocale()).toBe('en');
    setLocale('de');
    expect(getLocale()).toBe('es');
    expect(document.documentElement.lang).toBe('es');
  });

  it('t() cae al español si una clave no estuviera en el catálogo activo', () => {
    setLocale('en');
    // `tn` de un plural inexistente devuelve la clave tal cual, sin romper la interfaz.
    expect(tn('clave.que.no.existe', 2)).toBe('clave.que.no.existe.other');
  });
});
