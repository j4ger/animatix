export type HighlightRange = readonly [start: number, end: number, role: string];
export type SourceHighlighter = (source: string) => Promise<readonly HighlightRange[]>;

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

export function bindSourceHighlight(
  editor: HTMLTextAreaElement,
  mirror: HTMLElement,
  load: () => Promise<{highlightSource?: SourceHighlighter}>,
  decorate?: (rows: readonly HTMLDivElement[], lines: readonly string[], ranges: readonly HighlightRange[]) => void,
) {
  const frame = editor.parentElement!;
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
        decorate?.(rows, lines, ranges);
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
    render(sourceLines = editor.value.split('\n')) {
      revision++;
      lines = sourceLines;
      frame.removeAttribute('data-highlighted');
      const content = document.createDocumentFragment();
      rows = lines.map(text => {
        const row = document.createElement('div');
        row.className = 'source-guide-line';
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
      return rows;
    },
  };
}
