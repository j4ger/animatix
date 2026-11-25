import assert from 'node:assert/strict';
import {readFileSync, readdirSync} from 'node:fs';
import {join} from 'node:path';
import test from 'node:test';
import {fileURLToPath} from 'node:url';
import {FluentParser} from '@fluent/syntax';
import {bundle, translator} from '../src/lib/fluent.ts';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const web = 'web/app/src';
const gui = 'crates/animatix-gui/src';
const rustLocales = 'crates/animatix-i18n/locales';
const analyzer = 'crates/animatix-analyzer/src/completer';
const parser = 'crates/animatix-syntax/src/parser';
const read = path => readFileSync(join(root, path), 'utf8');

function variables(value) {
  const names = new Set();
  function visit(node) {
    if (!node || typeof node !== 'object') return;
    if (node.type === 'VariableReference') names.add(node.id.name);
    for (const child of Object.values(node)) visit(child);
  }
  visit(value);
  return [...names].sort();
}

function catalog(paths) {
  const entries = new Set();
  const contract = new Map();
  const parser = new FluentParser({withSpans: false});
  for (const path of paths) {
    for (const entry of parser.parse(read(path)).body) {
      assert.notEqual(entry.type, 'Junk', path + ' has invalid Fluent: ' + entry.content);
      if (entry.type !== 'Message' && entry.type !== 'Term') continue;
      const key = (entry.type === 'Term' ? '-' : '') + entry.id.name;
      assert.ok(!entries.has(key), path + ' repeats ' + key);
      entries.add(key);
      if (entry.value) contract.set(key, variables(entry.value));
      for (const attribute of entry.attributes) {
        const member = key + '.' + attribute.id.name;
        assert.ok(!contract.has(member), path + ' repeats ' + member);
        contract.set(member, variables(attribute.value));
      }
    }
  }
  return contract;
}

function sources(directory, extensions) {
  const paths = readdirSync(join(root, directory), {recursive: true});
  return paths.filter(path => extensions.some(ext => path.endsWith(ext)))
              .map(path => read(join(directory, path)));
}

function requiredKeys(source, prefix) {
  const pattern = new RegExp("['\"](" + prefix + "[a-z0-9]+(?:[-.][a-z0-9]+)*)['\"]", 'g');
  // The completion row ID belongs to egui's interactive state.
  const stableWidgetIds = new Set(['completion-item']);
  return new Set([...source.matchAll(pattern)].map(match => match[1])
    .filter(key => !stableWidgetIds.has(key) && !key.endsWith('.amx')));
}

for (const {name, directories, files, extra = () => [], source, prefix} of [
  {
    name: 'GUI and Analyzer',
    directories: rustLocales,
    files: ['gui.ftl'],
    source: [...sources(gui, ['.rs']), ...sources(analyzer, ['.rs']), ...sources(parser, ['.rs'])].join('\n'),
    prefix: '(?:gui|diagnostic|completion)-',
  },
  {
    name: 'Web',
    directories: web + '/locales',
    files: ['site.ftl', 'client.ftl'],
    extra: locale => ['web/embed/src/locales/' + locale + '.ftl'],
    source: [...sources(web, ['.astro', '.ts']), ...sources('web/embed/src', ['.js'])].join('\n'),
    prefix: 'web-',
  },
]) {
  test(name + ' has complete and equivalent Fluent contracts', () => {
    const paths = locale => [...files.map(file => join(directories, locale, file)), ...extra(locale)];
    const english = catalog(paths('en'));
    const chinese = catalog(paths('zh-Hans'));
    assert.deepEqual(chinese, english);

    for (const key of requiredKeys(source, prefix)) {
      assert.ok(english.has(key), 'Missing catalog key ' + key);
    }
    if (name === 'GUI and Analyzer') {
      for (const match of source.matchAll(/\brow\(\s*ui,\s*"[^"]+",\s*"(gui-settings-[^"]+)",\s*true/g)) {
        assert.ok(english.has(match[1] + '.description'), 'Missing description on ' + match[1]);
      }
    }
    for (const locale of ['en', 'zh-Hans']) {
      const messages = bundle(locale, ...paths(locale).map(read));
      const t = translator(messages, messages);
      for (const [key, args] of english) {
        if (key.startsWith('-')) continue;
        const result = t(key, Object.fromEntries(args.map(name => [name, 'G0测试'])));
        assert.notEqual(result, key, locale + ' cannot format ' + key);
      }
    }
  });
}

test('a missing message or attribute falls back to the English pattern', () => {
  const english = bundle('en', 'only-en = English\ncontrol = Control\n    .hint = Open { $target }');
  const chinese = bundle('zh-Hans', 'control = 控件');
  const zh = translator(chinese, english);
  assert.equal(zh('control'), '控件');
  assert.equal(zh('control.hint', {target: 'scene.amx'}), 'Open scene.amx');
  assert.equal(zh('only-en'), 'English');
});
