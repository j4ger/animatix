/** @typedef {{t: number, kind: string, dur?: number, id?: string}} TimelineMarker */

/**
 * @param {HTMLElement} layer
 * @param {readonly TimelineMarker[]} markers
 * @param {number} duration
 */
export function renderTimelineMarks(layer, markers, duration) {
  const marks = document.createDocumentFragment();
  for (const { t, kind, dur = 0 } of markers) {
    if (kind === "cue") continue;
    const frac = duration > 0 ? Math.min(t / duration, 1) : 0;
    const mark = document.createElement("div");
    mark.className = kind === "transition" && dur > 0 ? "span"
      : kind === "scene" ? "diamond" : kind === "step" ? "step" : "tick";
    mark.dataset.t = String(t);
    mark.style.left = `${frac * 100}%`;
    if (mark.className === "span") {
      mark.style.width = `${Math.min(dur / duration, 1 - frac) * 100}%`;
    }
    marks.appendChild(mark);
  }
  layer.replaceChildren(marks);
}

/**
 * Snap only within six screen pixels, capped at 0.2 seconds.
 * @param {readonly TimelineMarker[]} markers
 * @param {number} time
 * @param {number} duration
 * @param {number} width
 */
export function findTimelineSnap(markers, time, duration, width) {
  const tolerance = Math.min(0.2, 6 * duration / width);
  let best = null;
  let distance = Infinity;
  for (const { t, kind, dur = 0 } of markers) {
    if (kind === "cue") continue;
    for (const point of kind === "transition" && dur > 0 ? [t, t + dur] : [t]) {
      const delta = Math.abs(time - point);
      if (delta <= tolerance && delta < distance) {
        best = point;
        distance = delta;
      }
    }
  }
  return best;
}

export const timelineStyles = `
.amx-transport {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: var(--amx-controls-height, 48px);
  padding-inline: var(--amx-controls-inset, 12px);
}

.amx-timeline {
  --amx-thumb-size: 12px;
  position: relative;
  display: grid;
  align-items: center;
  flex: 1;
  min-width: 0;
  height: 32px;
  padding-inline: calc(var(--amx-thumb-size) / 2);
  cursor: pointer;
}

.amx-rail {
  position: relative;
  height: 4px;
  border-radius: 99px;
  background: var(--amx-player-track, #394351);
}

.amx-fill, .amx-marks {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.amx-fill {
  border-radius: inherit;
  background: var(--amx-player-accent, #f5b942);
  transform: scaleX(0);
  transform-origin: left;
}

.amx-cursor, .amx-marks > :is(.tick, .step, .diamond) {
  position: absolute;
  top: 50%;
  transform: translate(-50%, -50%);
}

.amx-cursor {
  width: var(--amx-thumb-size);
  height: var(--amx-thumb-size);
  border-radius: 50%;
  background: var(--amx-player-accent, #f5b942);
  box-shadow: 0 0 0 2px var(--amx-player-surface, #0a0f17);
  pointer-events: none;
}

.amx-marks > :is(.tick, .step, .diamond) {
  width: 2px;
  height: 10px;
  border-radius: 1px;
  background: var(--amx-player-muted, #a7b3c0);
  box-shadow: 0 0 0 1px var(--amx-player-surface, #0a0f17);
}

.amx-marks > .diamond {
  width: 7px;
  height: 7px;
  transform: translate(-50%, -50%) rotate(45deg);
  background: #8ab4f8;
}

.amx-marks > .step { width: 3px; height: 12px; background: #34d399; }
.amx-marks > .snapped { background: var(--amx-player-accent, #f5b942); }
.amx-marks > .span {
  position: absolute;
  top: 0;
  bottom: 0;
  background: repeating-linear-gradient(135deg, #8ab4f888 0 4px, transparent 4px 8px);
}

@container (max-width: 28rem) {
  .amx-transport {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 0 12px;
  }
  .amx-transport .amx-timeline {
    --amx-thumb-size: 16px;
    grid-area: 1 / 1 / 2 / -1;
    height: 44px;
  }
  .amx-transport > button { min-height: 44px; }
  .amx-transport > .amx-time { justify-self: center; }
}
`;
