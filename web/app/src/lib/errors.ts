export function readableError(error:unknown):string {
 const message=error instanceof Error?error.message:String(error);
 if(/no WebGPU adapter|no suitable graphics adapter|webgpu.*not.*support/i.test(message))return 'This browser has no available WebGPU adapter. You can still edit and download the source for the native app.';
 return message;
}
