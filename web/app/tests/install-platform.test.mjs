import assert from 'node:assert/strict';
import test from 'node:test';
import {desktopPlatform} from '../src/lib/desktop-platform.ts';

test('detects the desktop OS from client hints or the legacy platform', () => {
  const browser = {platform: '', userAgent: '', maxTouchPoints: 0};
  assert.equal(desktopPlatform({...browser, platform: 'Linux x86_64'}), 'linux');
  assert.equal(desktopPlatform({...browser, platform: 'Win32'}), 'windows');
  assert.equal(desktopPlatform({...browser, platform: 'MacIntel'}), 'macos');
  assert.equal(desktopPlatform({...browser, platform: 'Darwin'}), 'macos');
  assert.equal(desktopPlatform({...browser, platform: 'Linux x86_64', userAgentData: {platform: 'Windows'}}), 'windows');
});

test('a mobile browser does not report a desktop installation target', () => {
  assert.equal(desktopPlatform({platform: 'MacIntel', userAgent: 'Safari', maxTouchPoints: 5}), undefined);
  assert.equal(desktopPlatform({platform: 'Linux arm', userAgent: 'Android', maxTouchPoints: 5}), undefined);
});
