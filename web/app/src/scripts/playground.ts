import {fetchSource} from '../lib/requests';
import {canonicalSceneId, overridesFor, parameterQueryNames, withOverride} from '../lib/overrides';
import {insertionId, insertionSteps} from '../lib/insertion-steps';
import {takeEditorTransfer, transferEditor} from '../lib/editor-transfer';
import {currentUi} from '../lib/ui';
import {editableScene} from './editable-scene';

const root = document.querySelector<HTMLElement>('[data-playground]');
if (root) installPlayground(root);

function installPlayground(root: HTMLElement) {
  const {t} = currentUi();
  const sample = root.querySelector<HTMLElement>('[data-sample]')!;
  const scene = editableScene(sample);
  const selector = root.querySelector<HTMLSelectElement>('#scene-choice')!;
  const defaultId = selector.value;
  const choices = new Map(Array.from(selector.options, option => [option.value, option]));
  const transferred = takeEditorTransfer('playground');
  let current = '';
  let leaving = false;

  async function openSource(id: string, initial = false) {
    const option = choices.get(id);
    const path = option?.dataset.source;
    if (!path) return;
    selector.value = id;
    scene.setReading(true);
    try {
      const next = new URL(location.href);
      const canonical = await fetchSource(path);
      const source = initial ? overridesFor(id, next.searchParams).reduce(
        (text, [name, value]) => withOverride(text, name, value), canonical,
      ) : canonical;
      const draft = initial && transferred && canonicalSceneId(transferred.scene) === id ? transferred : undefined;
      const step = initial && !draft && id === insertionId
        ? insertionSteps(source).find(move => move.id === next.searchParams.get('step')) : undefined;
      const requestedTime = Number(next.searchParams.get('t'));
      const time = step && (requestedTime >= step.start && requestedTime <= step.start + step.duration
        ? requestedTime : step.start + step.duration / 2);
      current = id;
      scene.replaceScene({id, path, title: option.text, canonical, source, draft, time});
      if (step) {
        const line = source.split('\n')[step.line - 1];
        const match = /\[([\d.]+(?:ms|s))/.exec(line);
        scene.selectLine(step.line, match ? match.index + 1 : 0, match?.[1].length);
      }
      next.searchParams.set('scene', id);
      if (!initial) [...parameterQueryNames, 'step', 't'].forEach(name => next.searchParams.delete(name));
      history.replaceState(null, '', next);
    } catch (error) {
      selector.value = current || id;
      scene.sourceError(error);
    }
  }

  sample.addEventListener('scene-controls', () => { selector.disabled = sample.hasAttribute('data-scene-busy'); });
  sample.addEventListener('scene-retry', () => void openSource(selector.value, true));
  selector.addEventListener('change', () => {
    const next = selector.value;
    if (next === current) return;
    if (scene.hasUnsaved() && !confirm(t('Replace your unsaved edits with another source?', '要用另一个场景替换尚未保存的修改吗？'))) {
      selector.value = current;
      return;
    }
    void openSource(next);
  });
  document.addEventListener('click', event => {
    const link = (event.target as Element)?.closest<HTMLAnchorElement>('a[href]');
    if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey ||
        !link || link.hasAttribute('download') || link.target === '_blank' || !scene.hasUnsaved()) return;
    const destination = new URL(link.href);
    if (destination.origin === location.origin && destination.pathname === location.pathname &&
        destination.search === location.search && destination.hash) return;
    if (!confirm(t('Leave this page? Download your source first to keep your edits.', '要离开这个页面吗？下载源码可以保留修改。'))) event.preventDefault();
    else leaving = true;
  });
  window.addEventListener('beforeunload', event => {
    if (!leaving && scene.hasUnsaved()) event.preventDefault();
  });
  window.addEventListener('pageshow', event => { if (event.persisted) leaving = false; });
  transferEditor('playground', () => scene.snapshot() ?? transferred, () => { leaving = true; });

  const requested = canonicalSceneId(new URLSearchParams(location.search).get('scene') ?? '');
  void openSource(choices.has(requested) ? requested : defaultId, true);
}
