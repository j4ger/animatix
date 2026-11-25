import {bundle, translator, type Translate} from './fluent';

interface ClientMessages {
  locale: string;
  messages: Record<string, string>;
  runtime: string;
  fallback: string;
}

let translate: Translate | undefined;

export function clientUi(): Translate {
  if (translate) return translate;
  const element = document.getElementById('animatix-ui');
  if (!element?.textContent) throw new Error('Missing Animatix UI catalog');
  const {locale, messages, runtime, fallback} = JSON.parse(element.textContent) as ClientMessages;
  const en = bundle('en', fallback);
  const format = translator(locale === 'en' ? en : bundle(locale, runtime), en);
  translate = (key, args) => Object.hasOwn(messages, key) ? messages[key] : format(key, args);
  return translate;
}
