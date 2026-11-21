export type Language = 'en' | 'zh';

export function pageLanguage(locale?: string): Language {
  return locale === 'zh' ? 'zh' : 'en';
}

export function htmlLanguage(language: Language) {
  return language === 'zh' ? 'zh-Hans' : 'en';
}

export function choose<T>(language: Language, english: T, chinese: T): T {
  return language === 'zh' ? chinese : english;
}

export function browserLanguage(preferences: readonly string[]): Language {
  for (const preference of preferences) {
    const code = preference.toLowerCase().split('-')[0];
    if (code === 'zh' || code === 'en') return code;
  }
  return 'en';
}

export function sitePagePath(pathname: string, base: string) {
  const route = base && pathname.startsWith(base + '/') ? pathname.slice(base.length) : pathname;
  return route.replace(/^\/zh(?=\/|$)/, '').replace(/^\//, '');
}

export function localizedPath(base: string, language: Language, path = '') {
  return base + (language === 'zh' ? '/zh/' : '/') + path.replace(/^\//, '');
}

export const languagePreference = 'animatix.language';
export const themePreference = 'animatix.theme';
