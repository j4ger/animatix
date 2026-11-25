import { readFileSync } from "node:fs";
import { basename } from "node:path";
import { FluentBundle, FluentResource } from "@fluent/bundle";

export function compileLabels(path) {
  const locale = basename(path, ".ftl");
  const resource = new FluentResource(readFileSync(path, "utf8"));
  const bundle = new FluentBundle(locale, { useIsolating: false });
  const errors = bundle.addResource(resource);
  if (errors.length) throw new AggregateError(errors, path);
  const entries = resource.body.map(({ id }) => {
    const { value } = bundle.getMessage(id);
    const formatted = bundle.formatPattern(value, null, errors);
    return [id, formatted];
  });
  if (errors.length) throw new AggregateError(errors, path);
  return Object.fromEntries(entries);
}
