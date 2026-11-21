import {desktopPlatform} from '../lib/desktop-platform';

const install = document.querySelector<HTMLElement>('[data-install]');
if (install) {
  const tabs = [...install.querySelectorAll<HTMLButtonElement>('[role=tab]')];
  const panels = [...install.querySelectorAll<HTMLElement>('[role=tabpanel]')];
  const platformName = desktopPlatform(navigator);

  function select(tab: HTMLButtonElement) {
    for (const item of tabs) {
      item.tabIndex = item === tab ? 0 : -1;
      item.setAttribute('aria-selected', String(item === tab));
    }
    for (const panel of panels) panel.hidden = panel.id !== tab.getAttribute('aria-controls');
  }

  select(tabs.find(tab => tab.dataset.platform === platformName) ?? tabs[0]);
  tabs.forEach((tab, index) => {
    tab.addEventListener('click', () => select(tab));
    tab.addEventListener('keydown', event => {
      const keys: Record<string, number> = {ArrowLeft: index - 1, ArrowRight: index + 1, Home: 0, End: tabs.length - 1};
      if (!Object.hasOwn(keys, event.key)) return;
      event.preventDefault();
      const next = tabs[(keys[event.key] + tabs.length) % tabs.length];
      select(next);
      next.focus();
    });
  });
}
