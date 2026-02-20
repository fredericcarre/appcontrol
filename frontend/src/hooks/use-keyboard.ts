import { useEffect } from 'react';

interface ShortcutMap {
  [key: string]: () => void;
}

export function useKeyboard(shortcuts: ShortcutMap) {
  useEffect(() => {
    function handler(e: KeyboardEvent) {
      const target = e.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) {
        return;
      }

      const key = [
        e.ctrlKey || e.metaKey ? 'Ctrl+' : '',
        e.shiftKey ? 'Shift+' : '',
        e.altKey ? 'Alt+' : '',
        e.key,
      ].join('');

      const action = shortcuts[key];
      if (action) {
        e.preventDefault();
        action();
      }
    }

    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [shortcuts]);
}
