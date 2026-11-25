import {editableScene} from './editable-scene';
import {clientUi} from '../lib/client-ui';

const story = document.querySelector<HTMLElement>('.journey')!;
const film = story.querySelector<HTMLElement>('.film')!;
const sample = film.querySelector<HTMLElement>('[data-sample]')!;
const scene = editableScene(sample);
const sections = [...story.querySelectorAll<HTMLElement>('[data-amx-cue]')];
const names = sections.map(section => section.dataset.amxCue!);
const cueStatus = sample.querySelector<HTMLElement>('[data-cue-status]')!;
const t = clientUi();
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
const narrow = matchMedia('(max-width: 740px)');
let scrollStops: number[] = [];
let cues: number[] = [];
let followingScroll = true;
let scrollFrame = 0;

function updateCues() {
  const markers = scene.timelineMarkers;
  if (!markers) {
    cues = [];
    cueStatus.textContent = '';
    return;
  }
  const times = new Map<string, number>();
  for (const marker of markers) {
    if (marker.kind === 'cue' && marker.id) times.set(marker.id, marker.t);
  }
  const missing = names.filter(name => !times.has(name));
  cues = missing.length ? [] : names.map(name => times.get(name)!);
  cueStatus.textContent = missing.length ? t('web-client-missing-cues', {cues: missing.join(document.documentElement.lang === 'zh-Hans' ? '、' : ', ')}) : '';
  scheduleScroll();
}

function scheduleScroll() {
  if (!scrollFrame) scrollFrame = requestAnimationFrame(followScroll);
}

// Map each passage's reading position to a frame in the same scene.
function resize() {
  const maxScroll = Math.max(0, document.documentElement.scrollHeight - innerHeight);
  const filmHeight = film.offsetHeight;
  story.style.setProperty('--film-height', `${filmHeight}px`);
  scrollStops = sections.map((section, i) => {
    if (!i) return 0;
    const rect = section.getBoundingClientRect();
    const offset = narrow.matches ? -filmHeight - 20 : (rect.height - innerHeight) / 2;
    return Math.max(0, Math.min(maxScroll, rect.top + scrollY + offset));
  });
  scheduleScroll();
}

function followScroll() {
  scrollFrame = 0;
  if (!cues.length || !scrollStops.length || !followingScroll || sample.hasAttribute('data-source-open')) return;
  const y = Math.max(0, scrollY);
  for (let i = 1; i < scrollStops.length; i++) {
    if (y >= scrollStops[i]) continue;
    const p = (y - scrollStops[i - 1]) / (scrollStops[i] - scrollStops[i - 1]);
    const progress = reduced.matches ? Math.round(p) : p;
    scene.seek(cues[i - 1] + progress * (cues[i] - cues[i - 1]));
    return;
  }
  scene.seek(cues[cues.length - 1]);
}

sample.addEventListener('scene-interaction', () => { followingScroll = false; });
sample.addEventListener('scene-document', updateCues);
window.addEventListener('scroll', () => {
  if (sample.hasAttribute('data-source-open')) return;
  followingScroll = true;
  scheduleScroll();
}, {passive: true});
window.addEventListener('resize', resize);
new ResizeObserver(resize).observe(film);
reduced.addEventListener('change', scheduleScroll);

for (const link of document.querySelectorAll<HTMLAnchorElement>('[data-edit-film]')) {
  link.addEventListener('click', event => {
    if (event.button || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    if (link.dataset.editFilm === 'half') {
      try {
        void scene.changeParameter('speaker_level', 0.5);
      } catch {
        // The Playground link remains available if the current draft removed the binding.
        return;
      }
    }
    event.preventDefault();
    scene.showSource();
    sample.scrollIntoView({block: 'start', behavior: 'instant'});
  });
}

resize();
updateCues();
