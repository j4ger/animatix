import assert from 'node:assert/strict';
import test from 'node:test';
import {localizedPath, sitePagePath} from '../src/lib/language.ts';

test('localized page URLs preserve the deployment prefix and query or fragment', () => {
  for (const base of ['', '/animatix']) {
    for (const language of ['en', 'zh']) {
      const root = base + (language === 'zh' ? '/zh/' : '/');
      assert.equal(localizedPath(base, language, '#examples'), root + '#examples');
      const page = localizedPath(base, language, 'playground/?scene=insertion-sort&step=four');
      assert.equal(page, root + 'playground/?scene=insertion-sort&step=four');
      assert.equal(sitePagePath(new URL(page, 'https://example.test').pathname, base), 'playground/');
      assert.equal(sitePagePath(root, base), '');
    }
  }
});
