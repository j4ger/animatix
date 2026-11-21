const sceneParameters: Record<string, Record<string, readonly number[]>> = {
  'one-rule-many-forms': { preset: [0, 1] },
  'when-waves-meet': { phase_degrees: [0, 90, 180] },
  'quiet-carriage': { speaker_level: [0.5, 1] },
  'give-it-a-beat': { beat: [0.06, 0.12, 0.2], diagonal: [0, 1], stress: [0, 1, 2, 3] },
};

export const parameterNames = Object.values(sceneParameters).flatMap(Object.keys);

/** Change a declared numeric default in the canonical, editable AMX. */
export function withOverride(source: string, name: string, value: number): string {
  if (!parameterNames.includes(name) || !Number.isFinite(value)) throw Error('Unsupported scene preset');
  const pattern = new RegExp('^let ' + name + ' = [^\\r\\n]+', 'm');
  if (!pattern.test(source)) throw Error(`Source has no ${name} default`);
  return source.replace(pattern, `let ${name} = ${value}`);
}

export function overridesFor(id: string, query: URLSearchParams): [string, number][] {
  const result: [string, number][] = [];
  for (const [name, allowed] of Object.entries(sceneParameters[id] ?? {})) {
    const raw = query.get(name)?.trim();
    if (raw && allowed.includes(Number(raw))) result.push([name, Number(raw)]);
  }
  return result;
}
