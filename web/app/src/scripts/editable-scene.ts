import {bindRecording, closeRecording, loadPlayer, loadPlayerModule, resetPlayer, showPlaybackFailure} from './scene';
import {readableError} from '../lib/errors';
import {sourceCues} from '../lib/insertion-steps';
import {draftSelection, takeEditorTransfer, transferEditor, type EditorDraft} from '../lib/editor-transfer';
import {currentUi} from '../lib/ui';
import {bindTimeline} from '../lib/timeline';
import {bindSourceEditor, type SourceKeyframe} from '../lib/source-highlight';
import {numericDefault, withOverride} from '../lib/overrides';
import {withSharedLayout} from '../lib/portable-source';
import sharedLayout from '../../../content/templates/layout.amx?raw';

const labels = currentUi();
const {t} = labels;
const scenes = new WeakMap<HTMLElement, ReturnType<typeof bindEditableScene>>();

export function editableScene(sample: HTMLElement) {
  let scene = scenes.get(sample);
  if (!scene) {
    scene = bindEditableScene(sample);
    scenes.set(sample, scene);
  }
  return scene;
}

interface SceneSource {
  id: string;
  path: string;
  title: string;
  canonical: string;
  source: string;
  time?: number;
  draft?: EditorDraft;
}

function bindEditableScene(sample: HTMLElement) {
  const shell = sample.querySelector<HTMLElement>('[data-scene-shell]')!;
  const controls = sample.querySelector<HTMLButtonElement>('.sample-play')!;
  const controlText = controls.querySelector('span')!;
  const slider = sample.querySelector<HTMLInputElement>('.timeline-input')!;
  const timeline = bindTimeline(slider, time => {
    signalInteraction();
    seek(time);
  });
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
  const filename = sample.querySelector<HTMLElement>('[data-scene-filename]')!;
  const mount = shell.querySelector<HTMLElement>('.scene-mount')!;
  const diagnostics = sample.querySelector<HTMLElement>('#diagnostics');
  const media = matchMedia('(prefers-reduced-motion: reduce)');
  const workspace = sample.hasAttribute('data-workspace');
  let id = workspace ? '' : sample.id;
  let original = editor.value;
  let canonical = original;
  let savedCopy: string | null = null;
  const transferKey = sample.hasAttribute('data-autoplay') ? 'home' : `example.${sample.id}`;
  const stored = workspace ? undefined : takeEditorTransfer(transferKey);
  const transferred = stored?.scene === id ? stored : undefined;
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
  let reading = workspace;
  let buildTimer: number | undefined;
  let buildPending = false;
  let replaySource: string | undefined;
  let downloadUrl: string | undefined;
  const hasUnsaved = () => editor.value !== original && editor.value !== savedCopy;
  const signalInteraction = () => sample.dispatchEvent(new Event('scene-interaction'));
  bindRecording(shell);

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
    updateDownload();
  }

  function updateDownload() {
    if (!download) return;
    if (downloadUrl) URL.revokeObjectURL(downloadUrl);
    downloadUrl = undefined;
    if (!id || reading) {
      download.removeAttribute('href');
      return;
    }
    const portable = withSharedLayout(editor.value, sharedLayout);
    if (portable !== canonical) downloadUrl = URL.createObjectURL(new Blob([portable], {type: 'text/plain'}));
    download.href = downloadUrl ?? shell.dataset.source!;
    download.download = `${id}.amx`;
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
    controls.disabled = busy || reading;
    slider.disabled = busy || reading || !player;
    reset.disabled = reading || !id || (editor.value === original && compiled === original);
    editor.readOnly = reading || !id;
    sample.toggleAttribute('data-scene-busy', busy || reading);
    if (preset) {
      preset.disabled = busy || !player;
      const value = String(numericDefault(compiled, preset.name));
      preset.value = [...preset.options].some(option => option.value === value) ? value : '';
    }
    sample.dispatchEvent(new Event('scene-controls'));
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

  function selectLine(line: number, from = 0, length?: number) {
    const lines = editor.value.split('\n');
    const start = lines.slice(0, line - 1).reduce((offset, text) => offset + text.length + 1, 0);
    if (matchMedia('(pointer: fine)').matches) editor.focus({preventScroll: true});
    editor.setSelectionRange(start + from, start + (length === undefined ? lines[line - 1]?.length ?? 0 : from + length));
    const style = getComputedStyle(editor);
    const measure = document.createElement('div');
    measure.className = 'source-wrap';
    Object.assign(measure.style, {
      position: 'fixed', visibility: 'hidden', width: editor.clientWidth + 'px',
      paddingLeft: style.paddingLeft, paddingRight: style.paddingRight,
      font: style.font, letterSpacing: style.letterSpacing,
    });
    measure.textContent = editor.value.slice(0, start + from) + '\u200b';
    document.body.append(measure);
    editor.scrollTop = Math.max(0, measure.getBoundingClientRect().height - 3 * parseFloat(style.lineHeight));
    measure.remove();
  }

  function clearDiagnostics() {
    status.textContent = '';
    diagnostics?.replaceChildren();
  }

  function showDiagnostics(error: unknown) {
    status.textContent = (workspace ? t('Not built:', '编译失败：') + ' ' : '') + readableError(error);
    if (!diagnostics) return;
    for (const item of (error as {diagnostics?: {line?: number; message?: string; code?: string}[]}).diagnostics ?? []) {
      const line = item.line;
      if (!line) continue;
      const button = document.createElement('button');
      button.className = 'diagnostic';
      button.type = 'button';
      button.textContent = `${labels.line(line)} · ${item.message ?? item.code ?? ''}`;
      button.addEventListener('click', () => selectLine(line));
      diagnostics.append(button);
    }
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
    compiled = canonical;
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
      if (previousPreview !== undefined && previousPreview !== canonical) {
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
      await buildDraft();
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
        showDiagnostics(error);
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

  function showSource() {
    signalInteraction();
    if (panel.hidden) setPanel(true);
  }

  function snapshot(): EditorDraft | undefined {
    if (!id) return;
    return {
      scene: id, source: editor.value, compiled, time: player?.time ?? selectedTime,
      selection: draftSelection(editor), scroll: editor.scrollTop, sourceOpen: !panel.hidden, savedCopy,
    };
  }

  let selectedTime: number | undefined;

  function setReading(value: boolean) {
    reading = value;
    updateControls();
    updateDownload();
  }

  function sourceError(error: unknown) {
    const message = t('Could not load source.', '无法加载源码。') + ' ' + readableError(error);
    shell.querySelector('.scene-status')!.textContent = message;
    if (!id) problem = true;
    paint();
    setReading(false);
  }

  function replaceScene(scene: SceneSource) {
    cancelQueuedBuild();
    pause();
    resetPlayer(shell);
    player = undefined;
    id = scene.id;
    shell.dataset.source = scene.path;
    shell.dataset.title = scene.title;
    mount.setAttribute('aria-label', labels.playback(scene.title));
    filename.textContent = `${id}.amx`;
    original = scene.source;
    compiled = canonical = scene.canonical;
    previousPreview = scene.draft?.compiled;
    savedCopy = scene.draft?.savedCopy ?? null;
    selectedTime = scene.draft?.time ?? scene.time;
    seconds = selectedTime ?? 0;
    editor.value = scene.draft?.source ?? original;
    editor.scrollTop = 0;
    clearDiagnostics();
    timeline.setDocument({duration: Math.max(1, seconds)});
    if (scene.draft) restoreEditor(scene.draft);
    reading = false;
    renderSource();
    paint();
    void start(selectedTime === undefined && !media.matches);
  }

  function restoreEditor(draft: EditorDraft) {
    editor.value = draft.source;
    if (draft.sourceOpen !== undefined) setPanel(draft.sourceOpen, false);
    editor.setSelectionRange(...draft.selection);
    editor.scrollTop = draft.scroll;
  }

  function changeParameter(name: string, value: number) {
    const source = withOverride(editor.value, name, value);
    cancelQueuedBuild();
    clearDiagnostics();
    if (presetStatus) presetStatus.textContent = '';
    editor.value = source;
    renderSource();
    replaySource = source;
    buildPending = true;
    return player ? buildDraft() : busy ? Promise.resolve() : start(false);
  }

  if (workspace) highlight.setVisible(true);
  inspect.addEventListener('click', () => {
    if (panel.hidden) showSource();
    else setPanel(false);
  });
  slider.addEventListener('change', revealCurrentCell);
  reset.addEventListener('click', () => {
    if (workspace && hasUnsaved() && !confirm(t('Restore the original source?', '要恢复原始源码吗？'))) return;
    cancelQueuedBuild();
    editor.value = original;
    editor.scrollTop = 0;
    clearDiagnostics();
    if (presetStatus) presetStatus.textContent = '';
    renderSource();
    updateControls();
    buildPending = true;
    void buildDraft();
  });
  editor.addEventListener('input', event => {
    renderSource();
    updateControls();
    clearDiagnostics();
    if (presetStatus) presetStatus.textContent = '';
    if (!(event as InputEvent).isComposing) queueBuild();
  });
  editor.addEventListener('compositionstart', cancelQueuedBuild);
  editor.addEventListener('compositionend', queueBuild);
  preset?.addEventListener('change', () => {
    if (!player || busy || !presetStatus) return;
    const hadFocus = document.activeElement === preset;
    try {
      void changeParameter(preset.name, Number(preset.value)).finally(() => {
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
    signalInteraction();
    if (!id) { sample.dispatchEvent(new Event('scene-retry')); return; }
    if (!player) { void start(true); return; }
    if (playing) seek(player!.time);
    else play();
  });
  media.addEventListener('change', () => { if (media.matches && playing) seek(player!.time); });
  shell.addEventListener('amxerror', event => {
    if (player) fail((event as CustomEvent).detail.error);
  });
  download?.addEventListener('click', () => {
    if (download.hasAttribute('href')) savedCopy = editor.value;
  });

  if (transferred) restoreEditor(transferred);
  if (!workspace) transferEditor(transferKey, snapshot);
  renderSource();
  updateControls();
  const autoplay = sample.hasAttribute('data-autoplay');
  if (id) void start(autoplay && !transferred && !media.matches);
  return {seek, showSource, selectLine, changeParameter, snapshot, hasUnsaved, setReading, sourceError, replaceScene};
}
