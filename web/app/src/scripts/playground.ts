import {fetchSource} from '../lib/requests';
import {readableError} from '../lib/errors';
import {overridesFor, parameterNames, withOverride} from '../lib/overrides';
import {loadPlayer, loadPlayerModule, resetPlayer} from './scene';
import {insertionId, insertionSteps} from '../lib/insertion-steps';
import {draftSelection, takeEditorTransfer, transferEditor} from '../lib/editor-transfer';
import {currentUi} from '../lib/ui';
import {bindSourceEditor} from '../lib/source-highlight';

const root = document.querySelector<HTMLElement>('[data-playground]');
if (root) installPlayground(root);

function installPlayground(root: HTMLElement) {
  const labels = currentUi();
  const {t} = labels;
  const code = root.querySelector<HTMLTextAreaElement>('#source-code')!;
  const selector = root.querySelector<HTMLSelectElement>('#scene-choice')!;
  const status = root.querySelector<HTMLElement>('#editor-status')!;
  const diagnostics = root.querySelector<HTMLElement>('#diagnostics')!;
  const shell = root.querySelector<HTMLElement>('[data-scene-shell]')!;
  const highlight = bindSourceEditor(code, () => loadPlayerModule(shell.dataset.module!));
  const reset = root.querySelector<HTMLButtonElement>('#reset')!;
  const download = root.querySelector<HTMLButtonElement>('#download')!;
  const retry = root.querySelector<HTMLButtonElement>('.retry-preview')!;
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
  const ids = Array.from(selector.options, option => option.value);
  const transferred = takeEditorTransfer('playground');
  let current = '';
  let canonical = '';
  let original = '';
  let compiled = '';
  let lastDownloaded: string | null = null;
  let startTime: number | undefined;
  let previousPreview: string | undefined;
  let leavingForLanguage = false;
  let player: Awaited<ReturnType<typeof loadPlayer>> | undefined;
  let sourceLoading = false;
  let building = false;
  let pending = false;
  let timer: number | undefined;

  function updateControls() {
    selector.disabled = sourceLoading || building;
    code.readOnly = sourceLoading;
    download.disabled = sourceLoading || !current;
    reset.disabled = sourceLoading || !current || code.value === original;
    retry.disabled = sourceLoading || building;
  }

  function setSelection(line: number, from = 0, length?: number) {
    const lines = code.value.split('\n');
    const start = lines.slice(0, line - 1).reduce((offset, text) => offset + text.length + 1, 0);
    if (matchMedia('(pointer: fine)').matches) code.focus({preventScroll: true});
    code.setSelectionRange(start + from, start + (length === undefined ? lines[line - 1]?.length ?? 0 : from + length));
    // Count the wrapped rows before the selection using the textarea's font
    // and client width, which excludes the scrollbar.
    const style = getComputedStyle(code);
    const measure = document.createElement('div');
    measure.className = 'source-wrap';
    Object.assign(measure.style, {
      position: 'fixed', visibility: 'hidden', width: code.clientWidth + 'px',
      paddingLeft: style.paddingLeft, paddingRight: style.paddingRight,
      font: style.font, letterSpacing: style.letterSpacing,
    });
    measure.textContent = code.value.slice(0, start + from) + '\u200b';
    document.body.append(measure);
    code.scrollTop = Math.max(0, measure.getBoundingClientRect().height - 3 * parseFloat(style.lineHeight));
    measure.remove();
  }

  function linkedPosition(query: URLSearchParams, text: string) {
    const step = insertionSteps(text).find(move => move.id === query.get('step'));
    if (!step) return undefined;
    const line = text.split('\n')[step.line - 1];
    const match = /\[([\d.]+(?:ms|s))/.exec(line);
    setSelection(step.line, match ? match.index + 1 : 0, match?.[1].length);
    const time = Number(query.get('t'));
    return time >= step.start && time <= step.start + step.duration ? time : step.start + step.duration / 2;
  }

  async function openSource(id: string, initial = false) {
    selector.value = id;
    sourceLoading = true;
    updateControls();
    status.textContent = labels.loadingSource;
    try {
      const next = new URL(location.href);
      const path = `${root.dataset.base}${id}.amx`;
      const source = await fetchSource(path);
      cancelQueuedBuild();
      canonical = source;
      original = initial ? overridesFor(id, next.searchParams).reduce(
        (text, [name, value]) => withOverride(text, name, value), source,
      ) : source;
      current = id;
      const draft = initial && transferred?.scene === id ? transferred : undefined;
      code.value = draft?.source ?? original;
      code.scrollTop = 0;
      compiled = canonical;
      previousPreview = draft?.compiled;
      lastDownloaded = draft?.savedCopy ?? null;
      diagnostics.replaceChildren();
      resetPlayer(shell);
      shell.dataset.source = path;
      player = undefined;
      startTime = draft ? draft.time : initial && id === insertionId ? linkedPosition(next.searchParams, original) : undefined;
      if (draft) {
        code.setSelectionRange(...draft.selection);
        code.scrollTop = draft.scroll;
      }
      highlight.render();
      highlight.setVisible(true);
      next.searchParams.set('scene', id);
      if (!initial) [...parameterNames, 'step', 't'].forEach(name => next.searchParams.delete(name));
      history.replaceState(null, '', next);
      status.textContent = '';
      pending = true;
    } catch (error) {
      selector.value = current || id;
      status.textContent = t('Could not load source.', '无法加载源码。') + ' ' + readableError(error);
      if (!current) retry.hidden = false;
    } finally {
      sourceLoading = false;
      updateControls();
      void buildDraft();
    }
  }

  function showDiagnostics(error: unknown) {
    status.textContent = t('Not built:', '编译失败：') + ' ' + readableError(error);
    for (const item of (error as {diagnostics?: {line?: number; message?: string; code?: string}[]}).diagnostics ?? []) {
      if (!item.line) continue;
      const button = document.createElement('button');
      button.className = 'diagnostic';
      button.type = 'button';
      button.textContent = `${labels.line(item.line)} · ${item.message ?? item.code ?? ''}`;
      button.addEventListener('click', () => {
        code.focus({preventScroll: true});
        setSelection(item.line!);
      });
      diagnostics.append(button);
    }
  }

  async function buildDraft() {
    if (!pending || building || sourceLoading || !current) return;
    pending = false;
    building = true;
    updateControls();
    const submitted = code.value;
    let clock = 0;
    try {
      if (!player?.isConnected) {
        player = await loadPlayer(shell);
        compiled = canonical;
        if (previousPreview !== undefined && previousPreview !== compiled) {
          await player.applySource(previousPreview);
          compiled = previousPreview;
        }
        previousPreview = undefined;
        player.seek(Math.min(startTime ?? 0, player.duration));
        if (startTime === undefined && !reducedMotion.matches) player.play();
        retry.hidden = true;
      }

      if (submitted === compiled) return;
      const preview = player;
      let time = preview.time;
      function followPlayhead() {
        time = preview.time;
        clock = requestAnimationFrame(followPlayhead);
      }
      followPlayhead();
      await preview.applySource(submitted);
      compiled = submitted;
      preview.seek(Math.min(time, preview.duration));
    } catch (error) {
      if (!player?.isConnected) retry.hidden = false;
      if (submitted === code.value) showDiagnostics(error);
    } finally {
      cancelAnimationFrame(clock);
      building = false;
      updateControls();
      void buildDraft();
    }
  }

  function cancelQueuedBuild() {
    window.clearTimeout(timer);
    timer = undefined;
    pending = false;
  }

  function queueBuild(delay = 350) {
    cancelQueuedBuild();
    status.textContent = '';
    diagnostics.replaceChildren();
    updateControls();
    timer = window.setTimeout(() => {
      timer = undefined;
      pending = true;
      void buildDraft();
    }, delay);
  }

  const hasUnsaved = () => code.value !== original && code.value !== lastDownloaded;
  selector.addEventListener('change', () => {
    const next = selector.value;
    if (next === current) return;
    if (hasUnsaved() && !confirm(t('Replace your unsaved edits with another source?', '要用另一个场景替换尚未保存的修改吗？'))) {
      selector.value = current;
      return;
    }
    void openSource(next);
  });
  reset.addEventListener('click', () => {
    if (hasUnsaved() && !confirm(t('Restore the original source?', '要恢复原始源码吗？'))) return;
    code.value = original;
    highlight.render();
    queueBuild(0);
  });
  retry.addEventListener('click', () => {
    if (current) queueBuild(0);
    else void openSource(selector.value, true);
  });
  shell.addEventListener('amxerror', () => {
    if (player && !player.isConnected) retry.hidden = false;
  });
  code.addEventListener('input', event => {
    highlight.render();
    if (!(event as InputEvent).isComposing) queueBuild();
  });
  code.addEventListener('compositionstart', cancelQueuedBuild);
  code.addEventListener('compositionend', () => queueBuild());
  download.addEventListener('click', () => {
    const url = URL.createObjectURL(new Blob([code.value], {type: 'text/plain'}));
    const link = document.createElement('a');
    link.href = url;
    link.download = `${current}.amx`;
    link.click();
    lastDownloaded = code.value;
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  });
  document.addEventListener('click', event => {
    const link = (event.target as Element)?.closest<HTMLAnchorElement>('a[href]');
    if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey ||
        !link || link.hasAttribute('download') || link.target === '_blank' || !hasUnsaved()) return;
    const destination = new URL(link.href);
    if (destination.origin === location.origin && destination.pathname === location.pathname &&
        destination.search === location.search && destination.hash) return;
    if (!confirm(t('Leave this page? Download your source first to keep your edits.', '要离开这个页面吗？下载源码可以保留修改。'))) event.preventDefault();
    else lastDownloaded = code.value;
  });
  window.addEventListener('beforeunload', event => {
    if (!leavingForLanguage && hasUnsaved()) event.preventDefault();
  });
  window.addEventListener('pageshow', event => { if (event.persisted) leavingForLanguage = false; });
  transferEditor('playground', () => current ? {
    scene: current, source: code.value, compiled, time: player?.time ?? startTime,
    selection: draftSelection(code), scroll: code.scrollTop, savedCopy: lastDownloaded,
  } : transferred, () => { leavingForLanguage = true; });

  const requested = new URLSearchParams(location.search).get('scene') ?? '';
  void openSource(ids.includes(requested) ? requested : 'first-move', true);
}
