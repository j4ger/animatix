import { cancellationError } from "./errors.js";

const MEDIA_TIMEOUT_MS = 15_000;

function hasSeekableRange(video) {
  const ranges = video.seekable;
  for (let index = 0; index < ranges.length; index++) {
    if (ranges.end(index) > ranges.start(index)) return true;
  }
  return false;
}

function waitForVideo(video, event, signal, start) {
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      reject(cancellationError());
      return;
    }
    const finish = (error) => {
      clearTimeout(timer);
      video.removeEventListener(event, ready);
      video.removeEventListener("error", failed);
      signal.removeEventListener("abort", aborted);
      if (error) reject(error);
      else resolve();
    };
    const ready = () => finish();
    const failed = () => finish(new Error(`Video '${video.src}': ${video.error?.message || "media could not be decoded"}`));
    const aborted = () => finish(cancellationError());
    const timer = setTimeout(() => finish(new Error(`Video '${video.src}' timed out waiting for ${event}`)), MEDIA_TIMEOUT_MS);
    video.addEventListener(event, ready, { once: true });
    video.addEventListener("error", failed, { once: true });
    signal.addEventListener("abort", aborted, { once: true });
    try { start(); } catch (error) { finish(error); }
  });
}

async function seekVideo(video, target, signal) {
  let notify;
  const presented = new Promise(resolve => { notify = resolve; });
  let started;
  let frameId;
  let paintId;
  let timeout;
  const observeFrame = () => {
    frameId = video.requestVideoFrameCallback((_now, frame) => {
      if (frame.presentationTime >= started && frame.mediaTime <= target + 0.000001) {
        notify();
      } else {
        observeFrame();
      }
    });
  };
  try {
    await waitForVideo(video, "seeked", signal, () => {
      started = performance.now();
      video.currentTime = target;
      if (typeof video.requestVideoFrameCallback === "function") observeFrame();
    });
    // The seek event can precede the compositor's frame update. If a seek
    // keeps the same source frame, some browsers skip the frame callback.
    if (signal.aborted) throw cancellationError();
    signal.addEventListener("abort", notify, { once: true });
    paintId = requestAnimationFrame(() => { paintId = requestAnimationFrame(notify); });
    timeout = setTimeout(notify, 100);
    await presented;
  } finally {
    signal.removeEventListener("abort", notify);
    if (frameId !== undefined) video.cancelVideoFrameCallback(frameId);
    if (paintId !== undefined) cancelAnimationFrame(paintId);
    clearTimeout(timeout);
  }
}

export async function loadVideo(url, signal) {
  const video = document.createElement("video");
  video.crossOrigin = "anonymous";
  video.muted = true;
  video.playsInline = true;
  video.preload = "auto";
  const lifetime = new AbortController();
  let objectUrl;
  const release = () => {
    if (lifetime.signal.aborted) return;
    lifetime.abort();
    video.removeAttribute("src");
    video.load();
    if (objectUrl) URL.revokeObjectURL(objectUrl);
  };
  const loadingSignal = signal ?? lifetime.signal;
  try {
    await waitForVideo(video, "loadeddata", loadingSignal, () => { video.src = url; });
    if (loadingSignal.aborted) throw cancellationError();
    if (!hasSeekableRange(video)) {
      // A plain static server may return 200 to Range requests. A local Blob
      // lets the browser seek even when it could only play the HTTP stream.
      const response = await fetch(url, { signal: loadingSignal });
      if (!response.ok) throw new Error(`Video '${url}' HTTP ${response.status}`);
      objectUrl = URL.createObjectURL(await response.blob());
      await waitForVideo(video, "loadeddata", loadingSignal, () => { video.src = objectUrl; });
      if (!hasSeekableRange(video)) throw new Error(`Video '${url}' cannot be seeked`);
    }
    const { videoWidth: width, videoHeight: height, duration } = video;
    if (!width || !height || width > 8192 || height > 8192 || width * height > 16_777_216 ||
        !Number.isFinite(duration) || duration <= 0) {
      throw new Error(`Video '${url}' has unsupported dimensions or duration`);
    }
    let context;
    let previous;
    let queue = Promise.resolve();
    async function capture(time, beforeTrimEnd) {
      if (lifetime.signal.aborted) throw cancellationError();
      // Browsers may round the next representable double back to the exact
      // trim boundary on their media clock. Keep a held sample inside it.
      const target = beforeTrimEnd ? Math.max(0, time - 0.000001) : time;
      if (previous?.time === target) return previous.pixels;
      if (video.seeking || video.currentTime !== target || video.readyState < 2) {
        await seekVideo(video, target, lifetime.signal);
      }
      if (lifetime.signal.aborted) throw cancellationError();
      if (!context) {
        const canvas = document.createElement("canvas");
        canvas.width = width;
        canvas.height = height;
        context = canvas.getContext("2d", { willReadFrequently: true });
        if (!context) throw new Error("Could not create a video sampling canvas");
      }
      context.drawImage(video, 0, 0, width, height);
      const pixels = context.getImageData(0, 0, width, height).data;
      previous = { time: target, pixels };
      return pixels;
    }
    return {
      metadata: { width, height, duration_s: duration },
      sample(time, beforeTrimEnd) {
        // A source can appear more than once in a composite frame, and GPU
        // recovery reuses media from the previous player until it is replaced.
        const next = queue.then(() => capture(time, beforeTrimEnd));
        queue = next.catch(() => {});
        return next;
      },
      release,
    };
  } catch (error) {
    release();
    throw error;
  }
}

export function releasePreparedBundle(bundle) {
  if (!bundle) return;
  bundle.prepared?.free?.();
  bundle.prepared = null;
  for (const video of bundle.videos?.values() ?? []) video.release();
  bundle.videos?.clear();
}
