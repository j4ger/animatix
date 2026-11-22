import {readableError} from '../lib/errors';
import {withTimeout} from '../lib/requests';
import {currentUi} from '../lib/ui';
import type {SourceHighlighter} from '../lib/source-highlight';
import type {TimelineMarker} from '../../../embed/src/player/timeline.js';

const {t} = currentUi();

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
    const imported = withTimeout(import(/* @vite-ignore */ module), 15000, t('Player loading timed out.', '播放器加载超时。'));
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
  shell.querySelector('.scene-status')!.textContent = t('Live playback unavailable.', '实时播放暂不可用。') + ' ' + readableError(error);
}

function playbackError(event: Event): Error {
  return new Error((event as CustomEvent).detail?.message || t('The runtime could not open this scene.', '运行时无法打开这个场景。'));
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
  status.textContent = t('Loading live playback…', '正在加载实时预览…');
  try {
    await loadPlayerModule(shell.dataset.module!);
    await withTimeout(customElements.whenDefined('amx-player'), 15000, t('Player registration timed out.', '播放器注册超时。'));

    player = document.createElement('amx-player') as Player;
    player.setAttribute('src', shell.dataset.source!);
    player.setAttribute('title', shell.dataset.title || t('Animation preview', '动画预览'));
    player.lang = document.documentElement.lang;
    player.setAttribute('aspect', shell.dataset.aspect!);
    for (const flag of ['sealed', 'no-veil', 'eager']) player.setAttribute(flag, '');
    const ready = new Promise<void>((resolve, reject) => {
      const options = {once: true, signal: listeners.signal};
      player!.addEventListener('amxready', () => resolve(), options);
      player!.addEventListener('amxerror', event => reject(playbackError(event)), options);
    });
    mount.append(player);
    await withTimeout(ready, 60000, t('Live playback took too long to start.', '实时播放启动超时。'));

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
        if (!recording.hidden && !video.error) status.textContent = t('Press play in the video controls to start the recording.', '点击视频控件上的播放按钮即可观看。');
      });
  });
  video.addEventListener('error', () => {
      if (recording.hidden) return;
      video.removeAttribute('src');
      status.textContent = t('The recording could not load. Try live playback again or download the scene source.', '无法加载录制视频。可以重试实时播放，或下载场景源码。');
  });
  new IntersectionObserver(([entry]) => {
    if (!entry.isIntersecting) video.pause();
  }).observe(shell);
}
