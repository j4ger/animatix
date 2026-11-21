import {sourceCells} from './source-cells';

export type HighlightRange = readonly [start: number, end: number, role: string];
export type SourceHighlighter = (source: string) => Promise<readonly HighlightRange[]>;
export type SourceKeyframe = {line: number; element: HTMLElement};

function colorLines(rows: HTMLElement[], lines: string[], ranges: readonly HighlightRange[]) {
  let start = 0;
  let index = 0;
  for (const [line, text] of lines.entries()) {
    const output = document.createDocumentFragment();
    const end = start + text.length;
    let cursor = start;
    while (index < ranges.length && ranges[index][0] < end) {
      const [from, to, role] = ranges[index];
      if (to > cursor) {
        const left = Math.max(from, cursor);
        const right = Math.min(to, end);
        output.append(text.slice(cursor - start, left - start));
        const token = document.createElement('span');
        token.dataset.role = role;
        token.textContent = text.slice(left - start, right - start);
        output.append(token);
        cursor = right;
      }
      if (to > end) break;
      index++;
    }
    output.append(text.slice(cursor - start));
    rows[line].firstElementChild!.replaceChildren(output);
    start = end + 1;
  }
}

function decorateCells(mirror: HTMLElement, rows: HTMLElement[], lines: string[], ranges: readonly HighlightRange[]) {
  const keyframes: SourceKeyframe[] = [];
  const content = document.createDocumentFragment();
  for (const {start, end, next, keyframe} of sourceCells(lines, ranges)) {
    const element = document.createElement('div');
    element.className = 'source-cell';
    if (keyframe) {
      rows[start].classList.add('keyframe-line');
      keyframes.push({line: start + 1, element});
    }
    for (let line = start; line < end; line++) element.append(rows[line]);
    content.append(element);
    for (let line = end; line < next; line++) content.append(rows[line]);
  }
  mirror.replaceChildren(content);
  return keyframes;
}

export function bindSourceEditor(
  editor: HTMLTextAreaElement,
  load: () => Promise<{highlightSource?: SourceHighlighter}>,
  onKeyframes?: (keyframes: SourceKeyframe[]) => void,
) {
  const frame = editor.parentElement!;
  const mirror = frame.querySelector<HTMLElement>('.source-guide')!;
  let rows: HTMLDivElement[] = [];
  let lines: string[] = [];
  let revision = 0;
  let raf: number | undefined;
  let enabled = false;
  let composing = false;
  let painting = false;

  function sync() {
    // clientWidth includes the textarea padding and excludes its scrollbar.
    mirror.style.width = editor.clientWidth + 'px';
    mirror.style.transform = `translateY(-${editor.scrollTop}px)`;
  }

  function schedule() {
    if (!enabled || composing || painting || raf !== undefined) return;
    raf = requestAnimationFrame(() => {
      raf = undefined;
      if (!enabled || composing) return;
      painting = true;
      const requested = revision;
      const source = editor.value;
      void load().then(sdk => sdk.highlightSource?.(source)).then(ranges => {
        if (!ranges || !enabled || composing || source !== editor.value || requested !== revision) return;
        colorLines(rows, lines, ranges);
        const keyframes = decorateCells(mirror, rows, lines, ranges);
        onKeyframes?.(keyframes);
        frame.setAttribute('data-highlighted', '');
      }).catch(() => {
        // A missing or offline SDK leaves the editable native text visible.
        if (requested === revision) frame.removeAttribute('data-highlighted');
      }).finally(() => {
        painting = false;
        if (requested !== revision) schedule();
      });
    });
  }

  editor.addEventListener('scroll', sync);
  new ResizeObserver(sync).observe(editor);
  editor.addEventListener('compositionstart', () => {
    composing = true;
    frame.removeAttribute('data-highlighted');
  });
  editor.addEventListener('compositionend', () => {
    composing = false;
    schedule();
  });

  return {
    sync,
    setVisible(visible: boolean) {
      enabled = visible;
      if (visible) schedule();
    },
    render() {
      revision++;
      lines = editor.value.split('\n');
      frame.style.setProperty('--source-line-digits', String(String(lines.length).length));
      frame.removeAttribute('data-highlighted');
      const content = document.createDocumentFragment();
      rows = lines.map((text, index) => {
        const row = document.createElement('div');
        row.className = 'source-guide-line';
        row.dataset.line = String(index + 1);
        const span = document.createElement('span');
        span.className = 'source-line-text';
        span.textContent = text;
        row.append(span);
        content.append(row);
        return row;
      });
      mirror.replaceChildren(content);
      sync();
      schedule();
    },
  };
}
