import { t, tn } from '$lib/i18n';

/** Tiempo relativo breve para la lista de recientes. */
export function relativeTime(iso: string, now: Date = new Date()): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return '';
  const diff = Math.max(0, now.getTime() - then);
  const min = Math.floor(diff / 60_000);
  if (min < 1) return t('time.now');
  if (min < 60) return t('time.minutes', { n: min });
  const h = Math.floor(min / 60);
  if (h < 24) return t('time.hours', { n: h });
  const d = Math.floor(h / 24);
  if (d === 1) return t('time.yesterday');
  if (d < 30) return t('time.days', { n: d });
  const mo = Math.floor(d / 30);
  if (mo < 12) return tn('time.months', mo);
  return tn('time.years', Math.floor(mo / 12));
}
