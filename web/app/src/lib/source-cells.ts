import type {HighlightRange} from './source-highlight';

// Use the tokenizer's ranges so a timestamp inside a string or comment never
// starts a card. Whitespace between cards still occupies its original rows.
export function sourceCells(lines: readonly string[], ranges: readonly HighlightRange[]) {
  const keyframes = new Set(ranges.filter(([, , role]) => role === 'keyframe').map(([start]) => start));
  const starts = [{line: 0, keyframe: false}];
  let offset = 0;
  for (const [line, text] of lines.entries()) {
    const indent = text.length - text.trimStart().length;
    if (keyframes.has(offset + indent)) {
      if (line === 0) starts[0].keyframe = true;
      else starts.push({line, keyframe: true});
    }
    offset += text.length + 1;
  }
  return starts.map(({line: start, keyframe}, index) => {
    const next = starts[index + 1]?.line ?? lines.length;
    let end = next;
    while (end > start + 1 && !lines[end - 1].trim()) end--;
    return {start, end, next, keyframe};
  });
}
