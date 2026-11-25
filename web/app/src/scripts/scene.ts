import {playbackError, readableError} from '../lib/errors';
import {withTimeout} from '../lib/requests';
import {clientUi} from '../lib/client-ui';
import type {SourceHighlighter} from '../lib/source-highlight';
import type {TimelineMarker} from '../../../embed/src/player/timeline.js';

const t = clientUi();

type Player = HTMLElement & {
  readonly time: number;
  readonly duration: number;
  readonly timelineMarkers?: readonly TimelineMarker[];
  play(): void;
  pause(): void;
  seek(time: number): void;
  applySource(source: string): Promise<unknown[]>;
  destroy(): void;
};

const loading = new WeakMap<HTMLElement, Promise<Player>>();
const imports = new Map<string, Promise<{highlightSource?: SourceHighlighter}>>();

export function loadPlayerModule(module: string) {
  let pending = imports.get(module);
  if (!pending) {
    const imported = withTimeout(import(/* @vite-ignore */ module), 15000, t('web-client-player-import-timeout'));
    pending = imported.catch(error => { imports.delete(module); throw error; });
    imports.set(module, pending);
  }
  return pending;
}

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

export function closeRecording(shell: HTMLElement) {
  const recording = shell.querySelector<HTMLElement>('.recording-panel');
  if (!recording) return;
  recording.querySelector('video')?.pause();
  recording.hidden = true;
  shell.querySelector<HTMLElement>('.scene-fallback')!.hidden = true;
  shell.querySelector<HTMLElement>('.scene-surface')!.hidden = false;
}

export function showPlaybackFailure(shell: HTMLElement, error: unknown) {
  const fallback = shell.querySelector<HTMLElement>('.scene-fallback');
  if (fallback) fallback.hidden = false;
  shell.querySelector('.scene-status')!.textContent = t('web-client-player-unavailable') + ' ' + readableError(error, t);
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
  status.textContent = t('web-client-player-loading');
  try {
    await loadPlayerModule(shell.dataset.module!);
    await withTimeout(customElements.whenDefined('amx-player'), 15000, t('web-client-player-registration-timeout'));

    player = document.createElement('amx-player') as Player;
    player.setAttribute('src', shell.dataset.source!);
    player.setAttribute('title', shell.dataset.title || t('web-client-player-preview'));
    player.lang = document.documentElement.lang;
    player.setAttribute('aspect', shell.dataset.aspect!);
    for (const flag of ['sealed', 'no-veil', 'eager']) player.setAttribute(flag, '');
    const ready = new Promise<void>((resolve, reject) => {
      const options = {once: true, signal: listeners.signal};
      player!.addEventListener('amxready', () => resolve(), options);
      player!.addEventListener('amxerror', event => {
        reject(playbackError((event as CustomEvent<unknown>).detail, t('web-client-player-scene-error')));
      }, options);
    });
    mount.append(player);
    await withTimeout(ready, 60000, t('web-client-player-ready-timeout'));

    shell.querySelector<HTMLElement>('.scene-placeholder')!.hidden = true;
    status.textContent = '';
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

export function bindRecording(shell: HTMLElement) {
  const recording = shell.querySelector<HTMLElement>('.recording-panel');
  if (!recording) return;
  const surface = shell.querySelector<HTMLElement>('.scene-surface')!;
  const status = shell.querySelector<HTMLElement>('.scene-status')!;
  const fallback = shell.querySelector<HTMLElement>('.scene-fallback')!;
  const video = recording.querySelector<HTMLVideoElement>('video')!;

  fallback.querySelector<HTMLButtonElement>('.watch-recording')!.addEventListener('click', () => {
      if (surface.getAttribute('aria-busy') === 'true') return;
      if (!video.hasAttribute('src')) video.src = video.dataset.recording!;
      surface.hidden = true;
      fallback.hidden = true;
      status.textContent = '';
      recording.hidden = false;
      video.focus({preventScroll: true});
      video.play().catch(() => {
        if (!recording.hidden && !video.error) status.textContent = t('web-client-player-recording-play-help');
      });
  });
  video.addEventListener('error', () => {
      if (recording.hidden) return;
      video.removeAttribute('src');
      status.textContent = t('web-client-player-recording-failed');
  });
  new IntersectionObserver(([entry]) => {
    if (!entry.isIntersecting) video.pause();
  }).observe(shell);
}
