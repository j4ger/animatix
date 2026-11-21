import {readableError} from '../lib/errors';
import {withOverride} from '../lib/overrides';
import {fetchSource, withTimeout} from '../lib/requests';

type Player = HTMLElement & {
  readonly time: number;
  readonly duration: number;
  play(): void;
  pause(): void;
  seek(time: number): void;
  applySource(source: string): Promise<unknown[]>;
  setVars(vars: Record<string, number>): Promise<void>;
  destroy(): void;
};

const loading = new WeakMap<HTMLElement, Promise<Player>>();
const imports = new Map<string, Promise<unknown>>();

export function loadPlayer(shell: HTMLElement): Promise<Player> {
  const pending = loading.get(shell);
  if (pending) return pending;
  const operation = createPlayer(shell).finally(() => loading.delete(shell));
  loading.set(shell, operation);
  return operation;
}

export function resetPlayer(shell: HTMLElement) {
  const player = shell.querySelector<Player>('amx-player');
  player?.destroy();
  player?.remove();
  shell.querySelector<HTMLElement>('.scene-placeholder')!.hidden = false;
  shell.querySelector('.scene-status')!.textContent = '';
}

function setPresetsEnabled(enabled: boolean) {
  document.querySelectorAll<HTMLElement>('[data-controls]').forEach(form => {
    form.querySelector('select')!.disabled = !enabled;
    if (enabled) form.querySelector('[role=status]')!.replaceChildren();
  });
}

function showPlaybackFailure(shell: HTMLElement, error: unknown) {
  const fallback = shell.querySelector<HTMLElement>('.scene-fallback');
  if (fallback) fallback.hidden = false;
  shell.querySelector('.scene-status')!.textContent = `Live playback unavailable. ${readableError(error)}`;
  setPresetsEnabled(false);
}

function playbackError(event: Event): Error {
  return new Error((event as CustomEvent).detail?.message || 'The runtime could not open this scene.');
}

async function createPlayer(shell: HTMLElement): Promise<Player> {
  const existing = shell.querySelector<Player>('amx-player');
  if (existing) return existing;

  const surface = shell.querySelector<HTMLElement>('.scene-surface')!;
  const mount = shell.querySelector<HTMLElement>('.scene-mount')!;
  const status = shell.querySelector<HTMLElement>('.scene-status')!;
  const listeners = new AbortController();
  let player: Player | undefined;
  surface.setAttribute('aria-busy', 'true');
  status.textContent = 'Loading live playback…';
  try {
    const module = shell.dataset.module!;
    if (!imports.has(module)) {
      const imported = withTimeout(import(/* @vite-ignore */ module), 15000, 'Player loading timed out.');
      imports.set(module, imported.catch(error => { imports.delete(module); throw error; }));
    }
    await imports.get(module);
    await withTimeout(customElements.whenDefined('amx-player'), 15000, 'Player registration timed out.');

    player = document.createElement('amx-player') as Player;
    player.setAttribute('src', shell.dataset.source!);
    player.setAttribute('title', shell.dataset.title || 'Animation preview');
    player.setAttribute('aspect', '16:9');
    if (shell.hasAttribute('data-inline')) {
      player.setAttribute('sealed', '');
      player.setAttribute('no-veil', '');
    } else {
      player.setAttribute('controls', '');
    }
    player.setAttribute('eager', '');
    const preset = document.querySelector<HTMLSelectElement>('[data-controls] select[data-applied-value]');
    if (preset) player.setAttribute('data-vars', JSON.stringify({[preset.name]: Number(preset.dataset.appliedValue)}));
    const ready = new Promise<void>((resolve, reject) => {
      const options = {once: true, signal: listeners.signal};
      player!.addEventListener('amxready', () => resolve(), options);
      player!.addEventListener('amxerror', event => reject(playbackError(event)), options);
    });
    mount.append(player);
    await withTimeout(ready, 60000, 'Live playback took too long to start.');

    shell.querySelector<HTMLElement>('.scene-placeholder')!.hidden = true;
    status.textContent = '';
    setPresetsEnabled(true);
    player.addEventListener('amxerror', event => {
      const error = playbackError(event);
      if ((event as CustomEvent).detail?.kind === 'capability') {
        resetPlayer(shell);
        showPlaybackFailure(shell, error);
      } else {
        status.textContent = readableError(error);
      }
    });
    return player;
  } catch (error) {
    if (player) resetPlayer(shell);
    status.textContent = '';
    throw error;
  } finally {
    listeners.abort();
    surface.removeAttribute('aria-busy');
  }
}

for (const shell of document.querySelectorAll<HTMLElement>('[data-scene-shell]')) {
  const surface = shell.querySelector<HTMLElement>('.scene-surface')!;
  const mount = shell.querySelector<HTMLElement>('.scene-mount')!;
  const status = shell.querySelector<HTMLElement>('.scene-status')!;
  const buttons = shell.querySelectorAll<HTMLButtonElement>('.load-scene');
  const fallback = shell.querySelector<HTMLElement>('.scene-fallback');
  const recording = shell.querySelector<HTMLElement>('.recording-panel');
  const video = recording?.querySelector<HTMLVideoElement>('video');

  for (const button of buttons) {
    button.addEventListener('click', async () => {
      const hadFocus = document.activeElement === button;
      buttons.forEach(action => { action.disabled = true; });
      video?.pause();
      if (recording) recording.hidden = true;
      if (fallback) fallback.hidden = true;
      surface.hidden = false;

      let focusTarget: HTMLElement | undefined;
      try {
        (await loadPlayer(shell)).play();
        focusTarget = mount;
      } catch (error) {
        showPlaybackFailure(shell, error);
        focusTarget = fallback?.querySelector<HTMLButtonElement>('.watch-recording')
          ?? surface.querySelector<HTMLButtonElement>('.load-scene') ?? undefined;
      } finally {
        buttons.forEach(action => { action.disabled = false; });
        if (hadFocus && (document.activeElement === button || document.activeElement === document.body)) {
          focusTarget?.focus({preventScroll: focusTarget === mount});
        }
      }
    });
  }

  if (recording && video && fallback) {
    fallback.querySelector<HTMLButtonElement>('.watch-recording')!.addEventListener('click', () => {
      if (surface.getAttribute('aria-busy') === 'true') return;
      if (!video.hasAttribute('src')) video.src = video.dataset.recording!;
      surface.hidden = true;
      fallback.hidden = true;
      status.textContent = '';
      recording.hidden = false;
      video.focus({preventScroll: true});
      video.play().catch(() => {
        if (!recording.hidden && !video.error) status.textContent = 'Press play in the video controls to start the recording.';
      });
    });
    video.addEventListener('error', () => {
      if (recording.hidden) return;
      video.removeAttribute('src');
      status.textContent = 'The recording could not load. Try live playback again or download the scene source.';
    });
    new IntersectionObserver(([entry]) => {
      if (!entry.isIntersecting) video.pause();
    }).observe(shell);
  }
}

for (const form of document.querySelectorAll<HTMLElement>('[data-controls]')) {
  const status = form.querySelector<HTMLElement>('[role=status]')!;
  let committed = form.querySelector<HTMLSelectElement>('select')?.value ?? '0';
  form.addEventListener('change', async event => {
    const input = event.target as HTMLSelectElement;
    const shell = document.querySelector<HTMLElement>('[data-scene-shell]')!;
    const player = shell.querySelector<Player>('amx-player');
    if (!player) {
      status.textContent = 'Start live playback before applying a setting.';
      return;
    }
    const hadFocus = document.activeElement === input;
    const value = Number(input.value);
    status.textContent = 'Applying…';
    try {
      input.disabled = true;
      const configured = withOverride(await fetchSource(shell.dataset.source!), input.name, value);
      await player.setVars({[input.name]: value});
      player.seek(0);
      player.play();
      const edit = document.querySelector<HTMLAnchorElement>('[data-open-code]');
      if (edit) {
        const next = new URL(edit.href);
        next.searchParams.set(input.name, String(value));
        edit.href = next.href;
      }
      const download = document.querySelector<HTMLAnchorElement>('[data-source-download]');
      if (download) {
        if (download.href.startsWith('blob:')) URL.revokeObjectURL(download.href);
        download.href = URL.createObjectURL(new Blob([configured], {type: 'text/plain'}));
      }
      committed = input.value;
      input.dataset.appliedValue = committed;
      status.textContent = `Applied ${input.name}: ${value}. Playing from the start. Code and download use this preset.`;
    } catch (error) {
      input.value = committed;
      status.textContent = `Not applied: ${readableError(error)}`;
    } finally {
      input.disabled = !player.isConnected;
      if (hadFocus && !input.disabled) input.focus();
    }
  });
}
