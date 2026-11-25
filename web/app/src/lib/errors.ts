import {isGraphicsBackendError} from '../../../embed/src/player/errors.js';
import type {Translate} from './fluent';

interface PlayerDiagnostic {
  line?: number;
  column?: number;
  span?: readonly [number, number];
  message?: string;
  code?: string;
}

/** Rust spans are UTF-8 bytes; textarea selection uses UTF-16 code units. */
export function diagnosticSelection(source: string, diagnostic: PlayerDiagnostic) {
  if (!diagnostic.line) return;
  const lines = source.split('\n');
  const lineStart = lines.slice(0, diagnostic.line - 1).reduce((start, line) => start + line.length + 1, 0);
  if (diagnostic.span) {
    const [from, to] = diagnostic.span;
    let bytes = 0, units = 0, start = 0;
    for (const ch of source) {
      if (bytes >= from && bytes >= to) break;
      const cp = ch.codePointAt(0)!;
      bytes += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
      units += ch.length;
      if (bytes <= from) start = units;
    }
    return {from: Math.max(0, start - lineStart), length: Math.max(0, units - start)};
  }
  if (diagnostic.column) {
    const line = [...(lines[diagnostic.line - 1] ?? '')];
    return {from: line.slice(0, diagnostic.column - 1).join('').length, length: line[diagnostic.column - 1]?.length ?? 0};
  }
}

interface PlaybackError extends Error {
  src?: string | null;
  kind?: string;
  diagnostics?: readonly PlayerDiagnostic[];
}

export function playbackError(detail: unknown, fallback: string): PlaybackError {
  const payload = detail && typeof detail === 'object'
    ? detail as {src?: unknown; message?: unknown; kind?: unknown; diagnostics?: unknown; error?: unknown}
    : {};
  const cause = payload.error instanceof Error ? payload.error : undefined;
  const message = typeof payload.message === 'string' && payload.message
    ? payload.message
    : cause?.message || fallback;
  const error: PlaybackError = new Error(message, {cause});
  if (typeof payload.src === 'string' || payload.src === null) error.src = payload.src;
  if (typeof payload.kind === 'string') error.kind = payload.kind;
  if (Array.isArray(payload.diagnostics)) error.diagnostics = payload.diagnostics;
  return error;
}

export function diagnosticsFrom(error: unknown): readonly PlayerDiagnostic[] {
  if (!error || typeof error !== 'object') return [];
  const {diagnostics} = error as PlaybackError;
  return Array.isArray(diagnostics) ? diagnostics : [];
}

export function readableError(error: unknown, t: Translate): string {
  const message = error instanceof Error ? error.message : String(error);
  const kind = error instanceof Error ? (error as PlaybackError).kind : undefined;
  // SDK versions before amxerror carried a kind exposed the underlying error.
  // Capability also covers features unrelated to graphics, such as gzip.
  if ((!kind || kind === 'capability') && isGraphicsBackendError(message)) {
    return t('web-player-graphics-unavailable');
  }
  switch (kind) {
    case 'capability':
      return t('web-client-error-capability') + ' ' + message;
    case 'version':
      return t('web-client-error-version') + ' ' + message;
    case 'resource':
      return t('web-client-error-resource') + ' ' + message;
    default:
      return message;
  }
}
