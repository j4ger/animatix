import assert from 'node:assert/strict';
import test from 'node:test';
import {diagnosticSelection} from '../src/lib/errors.ts';

test('diagnostic spans select UTF-16 across emoji, CRLF and multiple lines', () => {
  const source = '// 😀\r\nmsg: "中文🎬\nend';
  const start = source.indexOf('"');
  const bytes = Buffer.byteLength(source.slice(0, start));
  assert.deepEqual(diagnosticSelection(source, {line: 2, span: [bytes, Buffer.byteLength(source)]}), {
    from: 5, length: source.length - start,
  });
  assert.deepEqual(diagnosticSelection('😀x', {line: 1, column: 2}), {from: 2, length: 1});
});
