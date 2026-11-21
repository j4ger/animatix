import {findTimelineSnap, renderTimelineMarks, type TimelineMarker} from '../../../embed/src/player/timeline.js';

export function bindTimeline(slider: HTMLInputElement, onSeek: (time: number) => void) {
  const track = slider.closest('[data-timeline]')!;
  const rail = track.querySelector<HTMLElement>('.amx-rail')!;
  const fill = rail.querySelector<HTMLElement>('.amx-fill')!;
  const cursor = rail.querySelector<HTMLElement>('.amx-cursor')!;
  const marks = rail.querySelector<HTMLElement>('.amx-marks')!;
  let landmarks: readonly TimelineMarker[] = [];

  slider.addEventListener('input', () => {
    const raw = Number(slider.value);
    const width = rail.getBoundingClientRect().width;
    const snap = width > 0 && slider.matches(':active')
      ? findTimelineSnap(landmarks, raw, Number(slider.max), width) : null;
    onSeek(snap ?? raw);
  });

  return {
    setDocument(player: {readonly duration: number; readonly timelineMarkers?: readonly TimelineMarker[]}) {
      landmarks = player.timelineMarkers ?? [];
      slider.max = String(player.duration);
      renderTimelineMarks(marks, landmarks, player.duration);
    },
    paint(time: number) {
      slider.value = String(time);
      const frac = Number(slider.value) / Number(slider.max);
      fill.style.transform = `scaleX(${frac})`;
      cursor.style.left = `${frac * 100}%`;
    },
  };
}
