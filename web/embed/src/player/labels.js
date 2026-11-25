import en from "../locales/en.ftl";
import zh from "../locales/zh-Hans.ftl";

const controlLabels = {
  playback: "web-player-controls",
  play: "web-client-play",
  pause: "web-client-pause",
  replay: "web-client-replay",
  retry: "web-client-retry",
  timeline: "web-player-timeline",
  speed: "web-player-speed",
  scene: "web-player-scene",
  graphicsUnavailable: "web-player-graphics-unavailable",
  loadFailed: "web-player-load-failed",
  recoveryFailed: "web-player-recovery-failed",
};

function labels(preferred) {
  return Object.fromEntries(Object.entries(controlLabels).map(([name, id]) => [
    name, preferred[id] ?? en[id] ?? id,
  ]));
}

const english = labels(en);
const chinese = labels(zh);

export function playerLabels(locale = "") {
  return /^zh(?:-|$)/i.test(locale) ? chinese : english;
}
