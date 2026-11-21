export const insertionId = 'insertion-sort';

const moves = [
  {id: 'five', label: 'Shift 5', target: 'five.at'},
  {id: 'four', label: 'Shift 4', target: 'four.at'},
  {id: 'key', label: 'Insert 3', target: 'key.at'},
];

// This editor marks literal, absolute AMX keyframe times.
export function sourceCues(source: string) {
  return source.split('\n').flatMap((line, index) => {
    const match = /^\s*#((?:\d+(?:\.\d+)?|\.\d+)(ms|s))\s*(?:\/\/.*)?$/.exec(line);
    if (!match) return [];
    const time = parseFloat(match[1]) / (match[2] === 'ms' ? 1000 : 1);
    return [{line: index + 1, label: match[1], time}];
  });
}

// The final move of each card is the one the inspector follows.
// Read the authored cue, duration and line from the file visible to the visitor.
export function insertionSteps(source: string) {
  const lines = source.split('\n');
  const cues = sourceCues(source);
  return moves.flatMap(move => {
    const index = lines.findLastIndex(line => line.trimStart().startsWith(move.target + ' ='));
    if (index < 0) return [];
    const cue = cues.findLast(cue => cue.line <= index);
    if (!cue) return [];
    const timing = lines[index].match(/\[\s*([\d.]+)(ms|s)\b/);
    const duration = timing ? Number(timing[1]) / (timing[2] === 'ms' ? 1000 : 1) : 0;
    return [{...move, start: cue.time, duration, line: index + 1, code: lines[index].trim()}];
  });
}
