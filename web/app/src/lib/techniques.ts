// Techniques are creator intents, not a second feature reference.
export const techniques = [
  {
    id: 'reveal', file: 'technique-reveal', title: 'Bring an idea into view',
    goal: 'Let the viewer understand one relationship at a time.',
    explanation: 'The object appears first, then its name, then its connection. Fade-in reveals a filled object; draw-in traces a line. Explicit # markers make the reading order easy to edit.',
    exercise: 'Move the connection earlier. Does the viewer still have time to understand the point?',
    rule: 'The completed composition remains visible. An entrance is not an excuse to erase the ending.',
  },
  {
    id: 'focus', file: 'technique-focus', title: 'Direct attention',
    goal: 'Highlight one item without losing the surrounding context.',
    explanation: 'Opacity lowers the neighbours while colour and a small underline identify the active item. The underline gives a second cue besides colour. The layout never shifts.',
    exercise: 'Change which item receives the focus. Keep the underline and contrast changes synchronized.',
    rule: 'Use one clear hierarchy. Do not make every object pulse or compete at once.',
  },
  {
    id: 'stagger', file: 'technique-stagger', title: 'Build a rhythm',
    goal: 'Turn repeated entrances into a deliberate reading order.',
    explanation: 'stagger offsets five identical 600 ms fades by 300 ms each. The interval chooses when each entrance begins; it does not change the duration of the fade.',
    exercise: 'Try a 150 ms interval, then 600 ms. Compare overlap with one-at-a-time entrances.',
    rule: 'Time offsets should reveal structure, not simply make a scene busier.',
  },
  {
    id: 'labels', file: 'technique-labels', title: 'Keep a label attached',
    goal: 'Keep a moving subject and its annotation in a stable relationship.',
    explanation: 'One time-derived x position drives the point, the leader line and the label. Their vertical offsets remain fixed. Clamping progress leaves a readable resting frame after the motion.',
    exercise: 'Change the destination once in the shared expression. All three elements should still agree.',
    rule: 'Derive related geometry from shared values instead of copying independent keyframes.',
  },
];
