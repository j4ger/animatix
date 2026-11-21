const switchEvent = 'animatix:language-switch';

interface Draft {
  scene: string;
  source: string;
  compiled: string;
  time?: number;
  selection: [number, number];
  scroll: number;
  sourceOpen?: boolean;
  savedCopy?: string | null;
}

export function beforeLanguageSwitch() {
  return window.dispatchEvent(new Event(switchEvent, {cancelable: true}));
}

export function transferEditor(key: string, snapshot: () => Draft | undefined, ready?: () => void) {
  window.addEventListener(switchEvent, event => {
    try {
      const draft = snapshot();
      if (draft) sessionStorage.setItem('animatix.transfer.' + key, JSON.stringify(draft));
      ready?.();
    } catch {
      event.preventDefault();
    }
  });
}

export function takeEditorTransfer(key: string): Draft | undefined {
  const storageKey = 'animatix.transfer.' + key;
  try {
    const value = sessionStorage.getItem(storageKey);
    sessionStorage.removeItem(storageKey);
    if (value) return JSON.parse(value) as Draft;
  } catch {
    // Storage is optional. A transfer that could not be saved never navigates.
  }
}

export function draftSelection(editor: HTMLTextAreaElement): [number, number] {
  return [editor.selectionStart, editor.selectionEnd];
}
