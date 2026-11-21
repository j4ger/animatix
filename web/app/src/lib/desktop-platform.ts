type BrowserPlatform = Pick<Navigator, 'platform' | 'userAgent' | 'maxTouchPoints'> & {
  userAgentData?: {platform?: string};
};

export function desktopPlatform(browser: BrowserPlatform) {
  const platform = browser.userAgentData?.platform || browser.platform || browser.userAgent;
  if (/android|iphone|ipad|ipod/i.test(browser.userAgent) ||
      (browser.platform === 'MacIntel' && browser.maxTouchPoints > 1)) return undefined;
  if (/\b(?:mac|darwin)/i.test(platform)) return 'macos';
  if (/\bwin(?:dows|32|64)\b/i.test(platform)) return 'windows';
  if (/linux|x11/i.test(platform)) return 'linux';
  return undefined;
}
