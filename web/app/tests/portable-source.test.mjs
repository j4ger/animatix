import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';

import {withSharedLayout} from '../src/lib/portable-source.ts';

test('downloaded AMX includes its layout and retains edited scene data', () => {
  const layout = readFileSync(new URL('../../content/templates/layout.amx', import.meta.url), 'utf8');
  const draft = 'import "../templates/layout.amx"\n\nlet start_pattern = 1\n';
  const portable = withSharedLayout(draft, layout);
  assert.ok(portable.startsWith(layout.trimEnd()));
  assert.ok(portable.endsWith('\n\nlet start_pattern = 1\n'));
  assert.equal(withSharedLayout('let s = "example"\n', layout), 'let s = "example"\n');
});
