import {loadPlayer, resetPlayer} from './scene';
import {readableError} from '../lib/errors';
import {sourceCues} from '../lib/insertion-steps';

for (const sample of document.querySelectorAll<HTMLElement>('[data-sample]')) {
  const shell = sample.querySelector<HTMLElement>('[data-scene-shell]')!;
  const controls = sample.querySelector<HTMLButtonElement>('.sample-play')!;
  const controlText = controls.querySelector('span')!;
  const slider = sample.querySelector<HTMLInputElement>('.sample-time')!;
  const clock = sample.querySelector<HTMLOutputElement>('.sample-clock')!;
  const inspect = sample.querySelector<HTMLButtonElement>('.inspect')!;
  const panel = sample.querySelector<HTMLElement>('.source-panel')!;
  const editor = panel.querySelector('textarea')!;
  const timestamps = panel.querySelector<HTMLElement>('.source-cues')!;
  const guide = panel.querySelector<HTMLElement>('.source-guide')!;
  const reset = panel.querySelector<HTMLButtonElement>('.reset-source')!;
  const status = panel.querySelector<HTMLElement>('.source-status')!;
  const media = matchMedia('(prefers-reduced-motion: reduce)');
  const original = editor.value;
  let compiled = original;
  let cueButtons: HTMLButtonElement[] = [];
  let guideLines: HTMLDivElement[] = [];
  let cues: ReturnType<typeof sourceCues> = [];
  let activeCue = -1;
  let player: Awaited<ReturnType<typeof loadPlayer>> | undefined;
  let frame = 0;
  let playing = false;
  let seconds = 0;
  let problem = false;
  let busy = false;
  let buildTimer: number | undefined;
  let buildPending = false;

  function alignGuide() {
    guide.style.transform = `translateY(-${editor.scrollTop}px)`;
  }

  function sizeGuide() {
    // Match the textarea's usable width, including its padding and excluding
    // its scrollbar, so the guide wraps at the same character.
    guide.style.width = editor.clientWidth + 'px';
    alignGuide();
  }

  function markCurrentCue() {
    const index = editor.value === compiled ? cues.findLastIndex(cue => cue.time <= seconds + 0.001) : -1;
    if (index === activeCue) return;
    cueButtons[activeCue]?.removeAttribute('aria-current');
    if (activeCue >= 0) guideLines[cues[activeCue].line - 1].classList.remove('is-current');
    activeCue = index;
    cueButtons[index]?.setAttribute('aria-current', 'true');
    if (index >= 0) guideLines[cues[index].line - 1].classList.add('is-current');
  }

  function updateSourceCues() {
    cues = sourceCues(editor.value).sort((a, b) => a.time - b.time);
    const lines = editor.value.split('\n');
    const marked = new Set(cues.map(cue => cue.line));
    const fragment = document.createDocumentFragment();
    guideLines = lines.map((line, index) => {
      const row = document.createElement('div');
      row.className = 'source-guide-line';
      row.dataset.line = String(index + 1);
      row.classList.toggle('keyframe-line', marked.has(index + 1));
      const text = document.createElement('span');
      text.textContent = line;
      row.append(text);
      fragment.append(row);
      return row;
    });
    guide.replaceChildren(fragment);
    cueButtons = cues.map(cue => {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = '#' + cue.label;
      button.title = `Line ${cue.line}`;
      button.addEventListener('click', () => {
        if (player && !busy && editor.value === compiled) seek(cue.time);
        const beginning = lines.slice(0, cue.line - 1).join('\n').length + (cue.line > 1 ? 1 : 0);
        editor.setSelectionRange(beginning, beginning + lines[cue.line - 1].length);
        editor.scrollTop = guideLines[Math.max(0, cue.line - 3)].offsetTop;
        alignGuide();
      });
      return button;
    });
    timestamps.replaceChildren(...cueButtons);
    activeCue = -1;
    markCurrentCue();
    sizeGuide();
  }

  function paint() {
    const duration = Number(slider.max);
    slider.value = String(seconds);
    slider.style.setProperty('--progress', (duration ? 100 * seconds / duration : 0) + '%');
    clock.value = seconds.toFixed(2) + 's';
    controls.toggleAttribute('data-playing', playing);
    controlText.textContent = problem ? 'Retry' : playing ? 'Pause' : seconds >= duration ? 'Replay' : 'Play';
    if (!panel.hidden) markCurrentCue();
  }

  function updateControls() {
    controls.disabled = busy;
    slider.disabled = busy || !player;
    reset.disabled = editor.value === original && compiled === original;
  }

  function pause() {
    cancelAnimationFrame(frame);
    player?.pause();
    playing = false;
  }

  function seek(time: number) {
    pause();
    seconds = Math.min(Math.max(0, time), Number(slider.max));
    player?.seek(seconds);
    paint();
  }

  function tick() {
    if (!player || !playing) return;
    seconds = player.time;
    if (seconds >= player.duration) pause();
    paint();
    if (playing) frame = requestAnimationFrame(tick);
  }

  function play() {
    if (!player) return;
    if (seconds >= player.duration) player.seek(0);
    playing = true;
    player.play();
    tick();
  }

  function setPanel(open: boolean) {
    panel.hidden = !open;
    sample.toggleAttribute('data-source-open', open);
    inspect.setAttribute('aria-expanded', String(open));
    inspect.querySelector('span')!.textContent = open ? 'Hide source' : 'View source';
    if (open) {
      sizeGuide();
      seek(player?.time ?? seconds);
      sample.scrollIntoView({block: 'start'});
      cueButtons[0]?.focus({preventScroll: true});
    } else {
      inspect.focus();
    }
  }

  function fail(error: unknown) {
    pause();
    player = undefined;
    resetPlayer(shell);
    compiled = original;
    problem = true;
    shell.querySelector('.scene-status')!.textContent = readableError(error);
    paint();
    updateControls();
  }

  async function start() {
    problem = false;
    busy = true;
    updateControls();
    try {
      player = await loadPlayer(shell);
      slider.max = String(player.duration);
      seek(seconds);
      if (panel.hidden && !media.matches) play();
      if (buildTimer === undefined) buildPending = editor.value !== compiled;
    } catch (error) {
      fail(error);
    } finally {
      busy = false;
      updateControls();
      void buildDraft();
    }
  }

  async function buildDraft() {
    if (busy || !buildPending || !player) return;
    buildPending = false;
    const submitted = editor.value;
    if (submitted === compiled) return;
    const preview = player;
    busy = true;
    updateControls();
    try {
      await preview.applySource(submitted);
      if (player !== preview) return;
      // applySource resets the runtime clock. Restore the visible playhead and
      // preserve its playback state without restarting the film.
      compiled = submitted;
      slider.max = String(preview.duration);
      seconds = Math.min(seconds, preview.duration);
      preview.seek(seconds);
      if (seconds >= preview.duration) pause();
      paint();
    } catch (error) {
      if (player === preview && editor.value === submitted) {
        status.textContent = readableError(error);
      }
    } finally {
      busy = false;
      updateControls();
      void buildDraft();
    }
  }

  function cancelQueuedBuild() {
    window.clearTimeout(buildTimer);
    buildTimer = undefined;
    buildPending = false;
  }

  function queueBuild() {
    cancelQueuedBuild();
    buildTimer = window.setTimeout(() => {
      buildTimer = undefined;
      buildPending = true;
      void buildDraft();
    }, 350);
  }

  inspect.addEventListener('click', () => setPanel(panel.hidden));
  reset.addEventListener('click', () => {
    cancelQueuedBuild();
    editor.value = original;
    editor.scrollTop = 0;
    status.textContent = '';
    updateSourceCues();
    updateControls();
    buildPending = true;
    void buildDraft();
  });
  editor.addEventListener('input', event => {
    updateSourceCues();
    updateControls();
    status.textContent = '';
    if (!(event as InputEvent).isComposing) queueBuild();
  });
  editor.addEventListener('scroll', alignGuide);
  new ResizeObserver(sizeGuide).observe(editor);
  editor.addEventListener('compositionstart', cancelQueuedBuild);
  editor.addEventListener('compositionend', queueBuild);
  slider.addEventListener('input', () => seek(Number(slider.value)));
  controls.addEventListener('click', () => {
    if (problem) { void start(); return; }
    if (playing) seek(player!.time);
    else play();
  });
  media.addEventListener('change', () => { if (media.matches && playing) seek(player!.time); });
  shell.addEventListener('amxerror', event => {
    if (player) fail((event as CustomEvent).detail.error);
  });

  updateSourceCues();
  void start();
}
