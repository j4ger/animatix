import {beforeLanguageSwitch} from '../lib/editor-transfer';
import {languagePreference, themePreference} from '../lib/language';

const root = document.querySelector<HTMLElement>('[data-site-preferences]');
if (root) {
  const menus = [...root.querySelectorAll('details')];
  const themes = [...root.querySelectorAll<HTMLButtonElement>('[data-theme-choice]')];

  function remember(key: string, value: string) {
    try {
      if (value === 'auto') localStorage.removeItem(key);
      else localStorage.setItem(key, value);
    } catch {
      // Apply the choice for this page even when storage is unavailable.
    }
  }

  function close(menu: HTMLDetailsElement) {
    menu.open = false;
    menu.querySelector('summary')?.focus({preventScroll: true});
  }

  function selectTheme(value: string) {
    if (value === 'auto') delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = value;
    for (const button of themes) button.setAttribute('aria-pressed', String(button.dataset.themeChoice === value));
  }

  selectTheme(document.documentElement.dataset.theme ?? 'auto');

  root.addEventListener('click', event => {
    const target = (event.target as Element).closest<HTMLElement>('[data-theme-choice], [data-language]');
    if (!target || !(event instanceof MouseEvent)) return;
    const menu = target.closest('details')!;
    const theme = target.dataset.themeChoice;
    if (theme) {
      selectTheme(theme);
      remember(themePreference, theme);
      close(menu);
      return;
    }
    if (!(target instanceof HTMLAnchorElement) || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    const language = target.dataset.language!;
    const next = new URL(target.href);
    next.search = location.search;
    next.hash = location.hash;
    if (next.href !== location.href && !beforeLanguageSwitch()) {
      alert(root.dataset.transferError);
      return;
    }
    remember(languagePreference, language);
    close(menu);
    if (next.href !== location.href) location.assign(next.href);
  });

  for (const menu of menus) {
    menu.addEventListener('toggle', () => {
      if (menu.open) for (const other of menus) if (other !== menu) other.open = false;
    });
    menu.addEventListener('keydown', event => {
      if (event.key === 'Escape' && menu.open) close(menu);
    });
  }
  document.addEventListener('click', event => {
    if (!root.contains(event.target as Node)) for (const menu of menus) menu.open = false;
  });
}
