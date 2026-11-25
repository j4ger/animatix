import {bundle, translator} from './fluent';
import {type Language} from './language';
import en from '../locales/en/site.ftl?raw';
import zh from '../locales/zh-Hans/site.ftl?raw';
import enPlayer from '../../../embed/src/locales/en.ftl?raw';
import zhPlayer from '../../../embed/src/locales/zh-Hans.ftl?raw';
import enRuntime from '../locales/en/client.ftl?raw';
import zhRuntime from '../locales/zh-Hans/client.ftl?raw';

const english = bundle('en', en, enPlayer, enRuntime);
const chinese = bundle('zh-Hans', zh, zhPlayer, zhRuntime);

export const ui = (language: Language) => {
  const t = translator(language === 'zh' ? chinese : english, english);
  return {
    t,
    examples: t('web-nav-examples'),
    docs: t('web-nav-docs'),
    playground: t('web-nav-playground'),
    tutorials: t('web-nav-tutorials'),
    techniques: t('web-nav-techniques'),
    reference: t('web-nav-reference'),
    quickstart: t('web-nav-quickstart'),
    play: t('web-client-play'),
    pause: t('web-client-pause'),
    replay: t('web-client-replay'),
    retry: t('web-client-retry'),
    reset: t('web-scene-reset'),
    viewSource: t('web-client-view-source'),
    hideSource: t('web-client-hide-source'),
    amxSource: t('web-scene-amx-source'),
    waitingForScene: t('web-scene-waiting'),
    playback: (title: string) => t('web-client-playback', {title}),
  };
};

const clientKeys = [
  'web-client-editor-build-failed',
  'web-client-editor-confirm-reset',
  'web-client-editor-source-failed',
  'web-client-error-capability',
  'web-client-error-resource',
  'web-client-error-version',
  'web-client-hide-source',
  'web-player-graphics-unavailable',
  'web-client-pause',
  'web-client-play',
  'web-client-player-import-timeout',
  'web-client-player-loading',
  'web-client-player-preview',
  'web-client-player-ready-timeout',
  'web-client-player-recording-failed',
  'web-client-player-recording-play-help',
  'web-client-player-registration-timeout',
  'web-client-player-scene-error',
  'web-client-player-unavailable',
  'web-client-playground-confirm-leave',
  'web-client-playground-confirm-replace',
  'web-client-replay',
  'web-client-retry',
  'web-client-view-source',
];

export function clientCatalog(language: Language) {
  const {t} = ui(language);
  return JSON.stringify({
    locale: language === 'zh' ? 'zh-Hans' : 'en',
    messages: Object.fromEntries(clientKeys.map(key => [key, t(key)])),
    runtime: language === 'zh' ? zhRuntime : enRuntime,
    fallback: enRuntime,
  }).replaceAll('<', '\\u003c');
}
