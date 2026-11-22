import {choose, type Language} from './language';

export const ui = (language: Language) => {
  const t = (english: string, chinese: string) => choose(language, english, chinese);
  return {
    t,
    examples: t('Examples', '示例'),
    docs: t('Docs', '文档'),
    playground: t('Playground', '在线编辑器'),
    tutorials: t('Tutorials', '教程'),
    techniques: t('Techniques', '动画技法'),
    reference: t('Reference', '语言参考'),
    quickstart: t('Quickstart', '快速入门'),
    play: t('Play', '播放'),
    pause: t('Pause', '暂停'),
    replay: t('Replay', '重播'),
    retry: t('Retry', '重试'),
    reset: t('Reset', '重置'),
    viewSource: t('View source', '查看源码'),
    hideSource: t('Hide source', '收起源码'),
    amxSource: t('AMX source', 'AMX 源码'),
    waitingForScene: t('Waiting for the scene…', '正在等待场景…'),
    line: (value: number) => t(`Line ${value}`, `第 ${value} 行`),
    playback: (title: string) => t(`${title} playback`, `${title}播放区`),
  };
};

export const currentUi = () => ui(document.documentElement.lang.startsWith('zh') ? 'zh' : 'en');
