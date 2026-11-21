import {closeRecording, loadPlayer, loadPlayerModule, resetPlayer, showPlaybackFailure} from './scene';
import {readableError} from '../lib/errors';
import {sourceCues} from '../lib/insertion-steps';
import {draftSelection, takeEditorTransfer, transferEditor} from '../lib/editor-transfer';
import {currentUi} from '../lib/ui';
import {bindTimeline} from '../lib/timeline';
import {bindSourceEditor, type SourceKeyframe} from '../lib/source-highlight';
import {numericDefault, withOverride} from '../lib/overrides';

const labels = currentUi();

for (const sample of document.querySelectorAll<HTMLElement>('[data-sample]')) {
  const shell = sample.querySelector<HTMLElement>('[data-scene-shell]')!;
  const controls = sample.querySelector<HTMLButtonElement>('.sample-play')!;
  const controlText = controls.querySelector('span')!;
  const slider = sample.querySelector<HTMLInputElement>('.timeline-input')!;
  const timeline = bindTimeline(slider, seek);
  const clock = sample.querySelector<HTMLOutputElement>('.sample-clock')!;
  const inspect = sample.querySelector<HTMLButtonElement>('.inspect')!;
  const panel = sample.querySelector<HTMLElement>('.source-panel')!;
  const editor = panel.querySelector('textarea')!;
  const guide = panel.querySelector<HTMLElement>('.source-guide')!;
  const highlight = bindSourceEditor(editor, () => loadPlayerModule(shell.dataset.module!), updateKeyframes);
  const reset = panel.querySelector<HTMLButtonElement>('.reset-source')!;
  const status = panel.querySelector<HTMLElement>('.source-status')!;
  const preset = sample.querySelector<HTMLSelectElement>('[data-scene-preset]');
  const presetStatus = sample.querySelector<HTMLElement>('.scene-presets [role=status]');
  const download = sample.querySelector<HTMLAnchorElement>('[data-source-download]');
  const originalDownload = download?.href;
  const media = matchMedia('(prefers-reduced-motion: reduce)');
  const original = editor.value;
  const transferKey = sample.hasAttribute('data-autoplay') ? 'home' : `example.${sample.id}`;
  const transferred = takeEditorTransfer(transferKey);
  let previousPreview = transferred?.compiled;
  let compiled = original;
  let timedCells: {element: HTMLElement; time: number}[] = [];
  let activeCell: HTMLElement | undefined;
  let player: Awaited<ReturnType<typeof loadPlayer>> | undefined;
  let frame = 0;
  let playing = false;
  let seconds = transferred?.time ?? 0;
  let problem = false;
  let busy = false;
  let buildTimer: number | undefined;
  let buildPending = false;
  let replaySource: string | undefined;

  function markCurrentCell() {
    const next = editor.value === compiled ? timedCells.findLast(cell => cell.time <= seconds + 0.001)?.element : undefined;
    if (next === activeCell) return;
    activeCell?.removeAttribute('data-current');
    activeCell = next;
    activeCell?.setAttribute('data-current', '');
  }

  function updateKeyframes(keyframes: SourceKeyframe[]) {
    const times = new Map(sourceCues(editor.value).map(cue => [cue.line, cue.time]));
    timedCells = keyframes.flatMap(({line, element}) => {
      const time = times.get(line);
      return time === undefined ? [] : [{element, time}];
    }).sort((a, b) => a.time - b.time);
    activeCell = undefined;
    markCurrentCell();
  }

  function renderSource() {
    timedCells = [];
    activeCell = undefined;
    highlight.render();
    if (download) {
      if (download.href.startsWith('blob:')) URL.revokeObjectURL(download.href);
      download.href = editor.value === original ? originalDownload! : URL.createObjectURL(new Blob([editor.value], {type: 'text/plain'}));
    }
  }

  function revealCurrentCell() {
    if (panel.hidden || !activeCell) return;
    const header = activeCell.firstElementChild!.getBoundingClientRect();
    const top = header.top - guide.getBoundingClientRect().top;
    if (top < editor.scrollTop || top + header.height > editor.scrollTop + editor.clientHeight) {
      editor.scrollTop = top;
      highlight.sync();
    }
  }

  function paint() {
    const duration = Number(slider.max);
    timeline.paint(seconds);
    clock.value = seconds.toFixed(2) + 's';
    controls.toggleAttribute('data-playing', playing);
    controlText.textContent = problem ? labels.retry : playing ? labels.pause : seconds >= duration ? labels.replay : labels.play;
    if (!panel.hidden) markCurrentCell();
  }

  function updateControls() {
    controls.disabled = busy;
    slider.disabled = busy || !player;
    reset.disabled = editor.value === original && compiled === original;
    if (preset) {
      preset.disabled = busy || !player;
      const value = String(numericDefault(compiled, preset.name));
      preset.value = [...preset.options].some(option => option.value === value) ? value : '';
    }
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

  function setPanel(open: boolean, interactive = true) {
    const anchor = interactive ? inspect.getBoundingClientRect().top : 0;
    panel.hidden = !open;
    highlight.setVisible(open);
    sample.toggleAttribute('data-source-open', open);
    inspect.setAttribute('aria-expanded', String(open));
    inspect.querySelector('span')!.textContent = open ? labels.hideSource : labels.viewSource;
    if (!interactive) return;
    if (open) {
      highlight.sync();
      seek(player?.time ?? seconds);
      panel.focus({preventScroll: true});
    } else {
      inspect.focus({preventScroll: true});
    }
    // Keep the controls at the same viewport position when the player resizes.
    const offset = inspect.getBoundingClientRect().top - anchor;
    if (Math.abs(offset) >= 1) window.scrollBy({top: offset, behavior: 'instant'});
  }

  function fail(error: unknown) {
    pause();
    if (player && compiled !== original) previousPreview = compiled;
    player = undefined;
    resetPlayer(shell);
    compiled = original;
    problem = true;
    showPlaybackFailure(shell, error);
    paint();
    updateControls();
  }

  async function start(autoplay: boolean) {
    const hadFocus = document.activeElement === controls;
    const sourceWasOpen = !panel.hidden;
    problem = false;
    busy = true;
    updateControls();
    closeRecording(shell);
    try {
      player = await loadPlayer(shell);
      if (previousPreview !== undefined && previousPreview !== original) {
        await player.applySource(previousPreview);
        compiled = previousPreview;
      }
      previousPreview = undefined;
      timeline.setDocument(player);
      seek(seconds);
      if (autoplay && (sourceWasOpen || panel.hidden)) play();
      if (buildTimer === undefined) buildPending = editor.value !== compiled;
    } catch (error) {
      fail(error);
    } finally {
      busy = false;
      updateControls();
      if (hadFocus && document.activeElement === document.body) {
        const fallback = shell.querySelector<HTMLButtonElement>('.scene-fallback:not([hidden]) .watch-recording');
        (fallback ?? controls).focus({preventScroll: true});
      }
      void buildDraft();
    }
  }

  async function buildDraft() {
    if (busy || !buildPending || !player) return;
    buildPending = false;
    const submitted = editor.value;
    const replay = submitted === replaySource;
    replaySource = undefined;
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
      timeline.setDocument(preview);
      if (replay && editor.value === submitted) {
        seek(0);
        play();
      } else {
        seconds = Math.min(seconds, preview.duration);
        preview.seek(seconds);
        if (seconds >= preview.duration) pause();
        paint();
      }
    } catch (error) {
      if (player === preview && editor.value === submitted) {
        status.textContent = readableError(error);
        if (replay && panel.hidden && presetStatus) presetStatus.textContent = status.textContent;
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
    replaySource = undefined;
  }

  function queueBuild() {
    cancelQueuedBuild();
    buildTimer = window.setTimeout(() => {
      buildTimer = undefined;
      buildPending = true;
      void buildDraft();
    }, 350);
  }

  inspect.addEventListener('click', () => {
    setPanel(panel.hidden);
    if (!panel.hidden && !player && !busy && !problem) void start(false);
  });
  slider.addEventListener('change', revealCurrentCell);
  reset.addEventListener('click', () => {
    cancelQueuedBuild();
    editor.value = original;
    editor.scrollTop = 0;
    status.textContent = '';
    if (presetStatus) presetStatus.textContent = '';
    renderSource();
    updateControls();
    buildPending = true;
    void buildDraft();
  });
  editor.addEventListener('input', event => {
    renderSource();
    updateControls();
    status.textContent = '';
    if (presetStatus) presetStatus.textContent = '';
    if (!(event as InputEvent).isComposing) queueBuild();
  });
  editor.addEventListener('compositionstart', cancelQueuedBuild);
  editor.addEventListener('compositionend', queueBuild);
  preset?.addEventListener('change', () => {
    if (!player || busy || !presetStatus) return;
    const hadFocus = document.activeElement === preset;
    try {
      const source = withOverride(editor.value, preset.name, Number(preset.value));
      cancelQueuedBuild();
      editor.value = source;
      status.textContent = presetStatus.textContent = '';
      renderSource();
      replaySource = source;
      buildPending = true;
      void buildDraft().finally(() => {
        if (hadFocus && !preset.disabled && document.activeElement === document.body) {
          preset.focus({preventScroll: true});
        }
      });
    } catch (error) {
      presetStatus.textContent = readableError(error);
      updateControls();
    }
  });
  controls.addEventListener('click', () => {
    if (!player) { void start(true); return; }
    if (playing) seek(player!.time);
    else play();
  });
  media.addEventListener('change', () => { if (media.matches && playing) seek(player!.time); });
  shell.addEventListener('amxerror', event => {
    if (player) fail((event as CustomEvent).detail.error);
  });

  if (transferred) {
    editor.value = transferred.source;
    setPanel(!!transferred.sourceOpen, false);
    editor.setSelectionRange(...transferred.selection);
    editor.scrollTop = transferred.scroll;
  }
  transferEditor(transferKey, () => ({
    scene: sample.id, source: editor.value, compiled, time: player?.time,
    selection: draftSelection(editor), scroll: editor.scrollTop, sourceOpen: !panel.hidden,
  }));
  renderSource();
  updateControls();
  if (sample.hasAttribute('data-autoplay') || transferred?.sourceOpen || transferred?.time !== undefined) {
    void start(!transferred && !media.matches);
  }
}
