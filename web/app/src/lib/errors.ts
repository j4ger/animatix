export function readableError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  if (/no WebGPU adapter|no suitable graphics adapter|webgpu.*not.*support|WebGL2 canvas:|no supported graphics backend/i.test(message)) {
    return 'This browser could not start WebGPU or WebGL2.';
  }
  return message;
}
