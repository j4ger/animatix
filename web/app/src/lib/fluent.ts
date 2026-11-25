import {FluentBundle, FluentResource, type FluentVariable} from '@fluent/bundle';

export type Translate = (key: string, args?: Record<string, FluentVariable>) => string;

export function bundle(locale: string, ...sources: string[]) {
  const result = new FluentBundle(locale, {useIsolating: false});
  for (const source of sources) {
    const errors = result.addResource(new FluentResource(source));
    if (errors.length) throw new AggregateError(errors, "Invalid Fluent catalog for " + locale);
  }
  return result;
}

export function translator(preferred: FluentBundle, english: FluentBundle): Translate {
  return (key, args) => {
    const dot = key.indexOf('.');
    const id = dot < 0 ? key : key.slice(0, dot);
    const attr = dot < 0 ? undefined : key.slice(dot + 1);
    const format = (catalog: FluentBundle) => {
      const message = catalog.getMessage(id);
      const pattern = attr ? message?.attributes[attr] : message?.value;
      if (pattern == null) return undefined;
      const errors: Error[] = [];
      const value = catalog.formatPattern(pattern, args, errors);
      return errors.length ? undefined : value;
    };
    return format(preferred) ?? (preferred === english ? undefined : format(english)) ?? key;
  };
}
