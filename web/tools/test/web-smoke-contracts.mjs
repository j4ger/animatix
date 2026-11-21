// Contracts name the artifact and user action, not legacy directories.
export function contracts(suite, depth = 'critical') {
  if (!['critical', 'deep'].includes(depth)) throw new Error(`Unknown smoke depth: ${depth}`);
  if (suite === 'sdk') return ['shapes', 'text', 'svg', 'shared', 'imports'].map(name => ({
    path: `/dev/smoke/?case=${name}`, action: 'fixture', name,
    players: name === 'shared' ? 2 : 1, fonts: name === 'text',
    motion: name === 'shapes' || name === 'shared' || name === 'imports',
    modules: name === 'imports' ? ['palette.amx', 'tokens.amx'] : [],
    paint: name === 'text' ? {color:'#F3F5F7',x:0.5,y:395/720,tolerance:0.08} : name === 'svg' ? {color:'#e5484d',x:0.5,y:0.47,tolerance:0.12} : undefined,
  }));
  if (suite !== 'site') throw new Error(`Unknown smoke suite: ${suite}`);
  const pages = [
    { path: '/', action: 'preview', players: 1, links: ['/#examples', '/docs/', '/playground/'] },
    { path: '/docs/', action: 'static', players: 0, links: ['/docs/quickstart/', '/learn/', '/docs/reference/'] },
    { path: '/docs/quickstart/', action: 'static', players: 0, links: ['/docs/', '/playground/'] },
    { path: '/learn/make-something-move/', action: 'load', players: 1, motion: true },
    { path: '/showcase/one-rule-many-forms/', action: 'load', players: 1, preset: true, recording: true },
    { path: '/showcase/one-rule-many-forms/?check=webgl2', action: 'load', players: 1, backend: 'webgl2', recording: true },
    { path: '/showcase/one-rule-many-forms/?check=recording', action: 'recording', players: 0, recording: true },
    { path: '/playground/?scene=first-move', action: 'edit', players: 1, motion: true },
  ];
  if (depth === 'deep') {
    const editor = pages.pop(); // Keep dirty-editor recovery last: navigation asks before discarding edits.
    for (const id of ['move-the-basis', 'make-room', 'when-waves-meet', 'words-keep-time']) {
      pages.push({path: `/showcase/${id}/`, action: 'load', players: 1, recording: true});
    }
    for (const id of ['compose-a-scene', 'repeat-with-intent', 'values-drive-motion', 'structure-time', 'finish-and-share']) {
      pages.push({path: `/learn/${id}/`, action: 'load', players: 1});
    }
    for (const id of ['focus', 'labels', 'reveal', 'stagger']) {
      pages.push({path: `/learn/techniques/${id}/`, action: 'load', players: 1});
    }
    pages.push(editor);
  }
  return pages;
}

export function siteBase(value = '/') {
  if (value === '' || value === '/') return '';
  if (!/^\/(?:[A-Za-z0-9_-]+\/)*[A-Za-z0-9_-]+\/?$/.test(value)) throw Error('SITE_BASE must be an absolute URL path prefix');
  return value.replace(/\/$/, '');
}
export function withBase(contract, prefix) {
  return {...contract,path:prefix+contract.path,links:contract.links?.map(path=>prefix+path)};
}
