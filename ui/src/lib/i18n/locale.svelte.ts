/**
 * Idioma activo de la interfaz.
 *
 * Vive en un módulo `.svelte.ts` (y no en `index.ts`) porque el idioma es estado reactivo:
 * al cambiarlo, `t()` devuelve otro texto y Svelte vuelve a pintar todo lo que lo llamó, sin
 * reiniciar la aplicación. El catálogo (`es`, `en`) se resuelve en `index.ts`.
 */

/** Idiomas disponibles en la interfaz. */
export type Locale = 'es' | 'en';

/** Idioma activo. Se lee desde `t()`/`tn()` y se cambia con `setLocale()`. */
export const localeState = $state<{ locale: Locale }>({ locale: 'es' });
