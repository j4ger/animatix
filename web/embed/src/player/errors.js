const KINDS = new Set(["resource", "capability", "version", "build", "cancel"]);

export function playerError(error, fallbackKind = "resource") {
  const kind = error?.name === "AbortError" ? "cancel"
    : (KINDS.has(error?.kind) ? error.kind : (KINDS.has(fallbackKind) ? fallbackKind : "resource"));
  let result = error;
  if (!(result instanceof Error)) result = new Error(String(error ?? "unknown player error"));
  try {
    result.kind = kind;
  } catch {
    const wrapped = new Error(result.message, { cause: result });
    wrapped.kind = kind;
    if (result.name === "AbortError") wrapped.name = "AbortError";
    result = wrapped;
  }
  if (result.diagnostics !== undefined && !Array.isArray(result.diagnostics)) {
    delete result.diagnostics;
  }
  return result;
}

export function playerErrorDetail(src, error, fallbackKind = "resource") {
  const normalized = playerError(error, fallbackKind);
  const detail = {
    src: src ?? null,
    kind: normalized.kind,
    message: normalized.message,
    error: normalized,
  };
  if (Array.isArray(normalized.diagnostics)) detail.diagnostics = normalized.diagnostics;
  return detail;
}

export function dispatchPlayerError(target, src, error, fallbackKind = "resource") {
  const normalized = playerError(error, fallbackKind);
  if (normalized.kind === "cancel") return null;
  const event = new CustomEvent("amxerror", {
    bubbles: true,
    detail: playerErrorDetail(src, normalized, fallbackKind),
  });
  target.dispatchEvent(event);
  return event;
}

export function cancellationError(message = "Operation cancelled", cause) {
  const error = new DOMException(message, "AbortError");
  error.kind = "cancel";
  if (cause !== undefined) error.cause = cause;
  return error;
}
