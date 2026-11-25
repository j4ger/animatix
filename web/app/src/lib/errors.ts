import {isGraphicsBackendError} from '../../../embed/src/player/errors.js';
import type {Translate} from './fluent';

interface PlayerDiagnostic {
  line?: number;
  message?: string;
  code?: string;
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
