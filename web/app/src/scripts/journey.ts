import {readableError} from '../lib/errors';
import {withTimeout} from '../lib/requests';
import {currentUi} from '../lib/ui';

const {t} = currentUi();

type LivePlayer = HTMLElement & {
  readonly time: number;
  readonly duration: number;
  play(): void;
  pause(): void;
  seek(seconds: number): void;
  destroy(): void;
};

const story = document.querySelector<HTMLElement>('.journey')!;
const film = story.querySelector<HTMLElement>('.film')!;
const sections = [...story.querySelectorAll<HTMLElement>('[data-cue]')];
const cues = sections.map(section => Number(section.dataset.cue));
const mount = film.querySelector<HTMLElement>('.engine-stage')!;
const opening = film.querySelector<HTMLImageElement>('.opening-frame')!;
const play = film.querySelector<HTMLButtonElement>('.play')!;
const playLabel = play.querySelector('span')!;
const slider = film.querySelector<HTMLInputElement>('.timeline')!;
const timecode = film.querySelector('output')!;
const status = film.querySelector<HTMLElement>('[role=status]')!;
const retry = film.querySelector<HTMLButtonElement>('.retry')!;
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
const narrow = matchMedia('(max-width: 740px)');
let engine: LivePlayer | undefined;
let scrollStops: number[] = [];
let seconds = 0;
let playing = false;
let followingScroll = true;
let clockFrame = 0;
let scrollFrame = 0;

function showTime(time: number) {
  seconds = time;
  slider.value = String(time);
  timecode.value = '0:' + String(Math.floor(time)).padStart(2, '0');
  playLabel.textContent = playing ? t('Pause film', '暂停动画') : time >= Number(slider.max) ? t('Replay film', '重播动画') : t('Play film', '播放动画');
  play.toggleAttribute('data-playing', playing);
}

function pause() {
  cancelAnimationFrame(clockFrame);
  engine?.pause();
  playing = false;
}

function seek(time: number) {
  if (playing) pause();
  if (time !== seconds) engine?.seek(time);
  showTime(time);
}

function trackPlayback() {
  if (!engine || !playing) return;
  if (engine.time >= engine.duration) pause();
  showTime(engine.time);
  if (playing) clockFrame = requestAnimationFrame(trackPlayback);
}

// Map each passage's reading position to a frame in the same film.
// Clamp to reachable scroll positions on short pages and wide screens.
function measureStops() {
  const maxScroll = Math.max(0, document.documentElement.scrollHeight - innerHeight);
  scrollStops = sections.map((section, i) => {
    if (!i) return 0;
    const rect = section.getBoundingClientRect();
    const offset = narrow.matches ? -film.offsetHeight - 20 : (rect.height - innerHeight) / 2;
    return Math.max(0, Math.min(maxScroll, rect.top + scrollY + offset));
  });
}

function followScroll() {
  scrollFrame = 0;
  if (!followingScroll) return;
  const y = Math.max(0, scrollY);
  for (let i = 1; i < scrollStops.length; i++) {
    if (y >= scrollStops[i]) continue;
    const p = (y - scrollStops[i - 1]) / (scrollStops[i] - scrollStops[i - 1]);
    const progress = reduced.matches ? Math.round(p) : p;
    seek(cues[i - 1] + progress * (cues[i] - cues[i - 1]));
    return;
  }
  seek(cues[cues.length - 1]);
}

function resize() {
  measureStops();
  if (followingScroll) followScroll();
}

play.addEventListener('click', () => {
  if (!engine) return;
  followingScroll = false;
  if (playing) {
    pause();
    showTime(engine.time);
    return;
  }
  if (engine.time >= engine.duration) engine.seek(0);
  playing = true;
  engine.play();
  trackPlayback();
});
slider.addEventListener('input', () => {
  followingScroll = false;
  seek(Number(slider.value));
});
window.addEventListener('scroll', () => {
  followingScroll = true;
  if (!scrollFrame) scrollFrame = requestAnimationFrame(followScroll);
}, {passive: true});
window.addEventListener('resize', resize);
reduced.addEventListener('change', () => {
  if (reduced.matches && playing) {
    pause();
    showTime(seconds);
  }
  if (followingScroll) followScroll();
});

function unavailable(error: unknown) {
  pause();
  engine?.destroy();
  engine = undefined;
  mount.replaceChildren();
  opening.hidden = false;
  play.disabled = slider.disabled = true;
  status.textContent = t('Showing the opening frame.', '正在显示开场静帧。') + ' ' + readableError(error);
  retry.hidden = false;
}

async function startEngine() {
  retry.hidden = true;
  status.textContent = t('Loading the film…', '正在加载动画…');
  const listeners = new AbortController();
  let player: LivePlayer | undefined;
  try {
    await withTimeout(import(/* @vite-ignore */ story.dataset.module!), 20000, 'The player could not load.');
    player = document.createElement('amx-player') as LivePlayer;
    player.setAttribute('src', story.dataset.source!);
    player.setAttribute('title', story.dataset.title!);
    player.lang = document.documentElement.lang;
    for (const flag of ['sealed', 'eager', 'no-veil']) player.setAttribute(flag, '');
    const ready = new Promise<void>((resolve, reject) => {
      player!.addEventListener('amxready', () => resolve(), {once: true, signal: listeners.signal});
      player!.addEventListener('amxerror', e => reject((e as CustomEvent).detail.error), {once: true, signal: listeners.signal});
    });
    mount.replaceChildren(player);
    await withTimeout(ready, 60000, t('The live film took too long to load.', '动画加载超时。'));
    engine = player;
    engine.addEventListener('amxerror', e => unavailable((e as CustomEvent).detail.error));
    engine.seek(seconds);
    slider.max = String(engine.duration);
    play.disabled = slider.disabled = false;
    opening.hidden = true;
    status.textContent = '';
  } catch (error) {
    if (player !== engine) player?.destroy();
    unavailable(error);
  } finally {
    listeners.abort();
  }
}

retry.addEventListener('click', () => void startEngine());
resize();
void startEngine();
