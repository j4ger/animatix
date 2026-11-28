import {lessonCopy, techniqueCopy, workCopy} from './catalog-zh';
import {lessons, works} from './content';
import {insertionId} from './insertion-steps';
import {techniques} from './techniques';
import type {Language} from './language';

export const noiseCancellation = {id: 'quiet-carriage', page: 'journey/'};
export const firstSceneId = lessons[0].file;

export function playgroundScenes(language: Language) {
  const title = (en: string, zh: string) => language === 'zh' ? zh : en;
  return [
    {id: insertionId, title: title('Insertion sort', '插入排序')},
    {id: noiseCancellation.id, title: title('Noise cancellation', '主动降噪')},
    ...lessons.map(x => ({id: x.file, title: title(x.title, lessonCopy[x.id].title)})),
    ...works.map(x => ({id: x.id, title: title(x.title, workCopy[x.id].title)})),
    ...techniques.map(x => ({id: x.file, title: title(x.title, techniqueCopy[x.id].title)})),
    {id: 'particle-lifecycle', title: title('One leaf, two clocks', '一片叶子的四秒')},
    {id: 'first-beat', title: title('First beat · wave only', '第一拍 · 波纹')},
    {id: 'give-it-a-beat', title: title('Give it a beat.', '给画面一点节奏')},
  ];
}
