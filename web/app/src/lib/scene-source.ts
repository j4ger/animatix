const sources = import.meta.glob<string>('../../../content/scenes/*.amx', {
  query: '?raw',
  import: 'default',
  eager: true,
});

export function sceneSource(id: string): string {
  const source = sources[`../../../content/scenes/${id}.amx`];
  if (source === undefined) throw new Error(`Missing AMX source for ${id}`);
  return source;
}
