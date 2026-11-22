// The published module is declaration-only so it can travel with an AMX draft.
const sharedLayoutImport = /^[\t ]*import[\t ]+"\.\.\/templates\/layout\.amx"[\t ]*(?=\r?$)/m;

export function withSharedLayout(source: string, layout: string): string {
  return source.replace(sharedLayoutImport, () => layout.trimEnd());
}
