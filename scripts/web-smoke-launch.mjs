// Linux CI needs a display-backed compositor even in headless mode. A WebGPU
// adapter alone does not prove that its canvas reaches Page.captureScreenshot.
export function chromiumLaunch(chrome, profile, port) {
  return {
    command: 'xvfb-run',
    args: ['-a', '-s', '-screen 0 1600x1200x24', chrome,
      '--headless=new', '--enable-gpu', '--no-sandbox',
      '--disable-dev-shm-usage', '--disable-gpu-sandbox',
      '--enable-unsafe-webgpu', '--ignore-gpu-blocklist',
      '--use-angle=swiftshader', '--enable-unsafe-swiftshader',
      '--enable-features=Vulkan', '--use-vulkan=swiftshader',
      '--no-first-run', '--no-default-browser-check', '--mute-audio',
      '--disable-background-networking', '--disable-extensions',
      `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, 'about:blank'],
  };
}
