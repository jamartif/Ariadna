export type Platform = 'mac' | 'windows' | 'linux';

/**
 * Plataforma del SO sin plugins: `?platform=mac|windows|linux` (revisión visual y
 * pruebas) o `navigator`. Fuera de Windows y macOS se asume Linux.
 */
export function detectPlatform(nav: Partial<Navigator> & { userAgentData?: { platform?: string } } = typeof navigator === 'undefined' ? {} : navigator, search = typeof location === 'undefined' ? '' : location.search): Platform {
  const forced = new URLSearchParams(search).get('platform');
  if (forced === 'mac' || forced === 'windows' || forced === 'linux') return forced;
  const p = `${nav.userAgentData?.platform ?? ''} ${nav.platform ?? ''} ${nav.userAgent ?? ''}`;
  if (/mac|iphone|ipad/i.test(p)) return 'mac';
  if (/win/i.test(p)) return 'windows';
  return 'linux';
}
