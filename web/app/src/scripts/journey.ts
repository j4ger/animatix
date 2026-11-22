import {editableScene} from './editable-scene';

const story = document.querySelector<HTMLElement>('.journey')!;
const film = story.querySelector<HTMLElement>('.film')!;
const sample = film.querySelector<HTMLElement>('[data-sample]')!;
const scene = editableScene(sample);
const sections = [...story.querySelectorAll<HTMLElement>('[data-cue]')];
const cues = sections.map(section => Number(section.dataset.cue));
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
const narrow = matchMedia('(max-width: 740px)');
let scrollStops: number[] = [];
let followingScroll = true;
let scrollFrame = 0;

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
  followScroll();
}

function followScroll() {
  scrollFrame = 0;
  if (!followingScroll || sample.hasAttribute('data-source-open')) return;
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
window.addEventListener('scroll', () => {
  if (sample.hasAttribute('data-source-open')) return;
  followingScroll = true;
  if (!scrollFrame) scrollFrame = requestAnimationFrame(followScroll);
}, {passive: true});
window.addEventListener('resize', resize);
new ResizeObserver(resize).observe(film);
reduced.addEventListener('change', followScroll);

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
