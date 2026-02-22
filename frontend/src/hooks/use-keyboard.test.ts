import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useKeyboard } from './use-keyboard';

describe('useKeyboard', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('should register keydown event listener on mount', () => {
    const addSpy = vi.spyOn(window, 'addEventListener');
    const shortcuts = { '?': vi.fn() };

    renderHook(() => useKeyboard(shortcuts));

    expect(addSpy).toHaveBeenCalledWith('keydown', expect.any(Function));
  });

  it('should remove keydown event listener on unmount', () => {
    const removeSpy = vi.spyOn(window, 'removeEventListener');
    const shortcuts = { '?': vi.fn() };

    const { unmount } = renderHook(() => useKeyboard(shortcuts));
    unmount();

    expect(removeSpy).toHaveBeenCalledWith('keydown', expect.any(Function));
  });

  it('should call action for matching key', () => {
    const action = vi.fn();
    const shortcuts = { '?': action };

    renderHook(() => useKeyboard(shortcuts));

    const event = new KeyboardEvent('keydown', { key: '?' });
    window.dispatchEvent(event);

    expect(action).toHaveBeenCalledTimes(1);
  });

  it('should not call action for non-matching key', () => {
    const action = vi.fn();
    const shortcuts = { '?': action };

    renderHook(() => useKeyboard(shortcuts));

    const event = new KeyboardEvent('keydown', { key: 'a' });
    window.dispatchEvent(event);

    expect(action).not.toHaveBeenCalled();
  });

  it('should handle Ctrl key combinations', () => {
    const action = vi.fn();
    const shortcuts = { 'Ctrl+f': action };

    renderHook(() => useKeyboard(shortcuts));

    const event = new KeyboardEvent('keydown', { key: 'f', ctrlKey: true });
    window.dispatchEvent(event);

    expect(action).toHaveBeenCalledTimes(1);
  });

  it('should handle Shift key combinations', () => {
    const action = vi.fn();
    const shortcuts = { 'Shift+A': action };

    renderHook(() => useKeyboard(shortcuts));

    const event = new KeyboardEvent('keydown', { key: 'A', shiftKey: true });
    window.dispatchEvent(event);

    expect(action).toHaveBeenCalledTimes(1);
  });

  it('should handle Alt key combinations', () => {
    const action = vi.fn();
    const shortcuts = { 'Alt+s': action };

    renderHook(() => useKeyboard(shortcuts));

    const event = new KeyboardEvent('keydown', { key: 's', altKey: true });
    window.dispatchEvent(event);

    expect(action).toHaveBeenCalledTimes(1);
  });

  it('should handle combined modifier keys', () => {
    const action = vi.fn();
    const shortcuts = { 'Ctrl+Shift+A': action };

    renderHook(() => useKeyboard(shortcuts));

    const event = new KeyboardEvent('keydown', { key: 'A', ctrlKey: true, shiftKey: true });
    window.dispatchEvent(event);

    expect(action).toHaveBeenCalledTimes(1);
  });

  it('should ignore events from INPUT elements', () => {
    const action = vi.fn();
    const shortcuts = { '?': action };

    renderHook(() => useKeyboard(shortcuts));

    const input = document.createElement('input');
    document.body.appendChild(input);

    const event = new KeyboardEvent('keydown', { key: '?', bubbles: true });
    Object.defineProperty(event, 'target', { value: input });
    window.dispatchEvent(event);

    expect(action).not.toHaveBeenCalled();

    document.body.removeChild(input);
  });

  it('should ignore events from TEXTAREA elements', () => {
    const action = vi.fn();
    const shortcuts = { '?': action };

    renderHook(() => useKeyboard(shortcuts));

    const textarea = document.createElement('textarea');
    document.body.appendChild(textarea);

    const event = new KeyboardEvent('keydown', { key: '?', bubbles: true });
    Object.defineProperty(event, 'target', { value: textarea });
    window.dispatchEvent(event);

    expect(action).not.toHaveBeenCalled();

    document.body.removeChild(textarea);
  });

  it('should ignore events from contentEditable elements', () => {
    const action = vi.fn();
    const shortcuts = { '?': action };

    renderHook(() => useKeyboard(shortcuts));

    const div = document.createElement('div');
    div.contentEditable = 'true';
    document.body.appendChild(div);

    const event = new KeyboardEvent('keydown', { key: '?', bubbles: true });
    Object.defineProperty(event, 'target', { value: div });
    window.dispatchEvent(event);

    expect(action).not.toHaveBeenCalled();

    document.body.removeChild(div);
  });

  it('should preventDefault on matched shortcuts', () => {
    const action = vi.fn();
    const shortcuts = { 'F5': action };

    renderHook(() => useKeyboard(shortcuts));

    const event = new KeyboardEvent('keydown', { key: 'F5', cancelable: true });
    const preventSpy = vi.spyOn(event, 'preventDefault');
    window.dispatchEvent(event);

    expect(preventSpy).toHaveBeenCalled();
    expect(action).toHaveBeenCalled();
  });

  it('should handle multiple shortcuts', () => {
    const action1 = vi.fn();
    const action2 = vi.fn();
    const shortcuts = { '?': action1, 'F5': action2 };

    renderHook(() => useKeyboard(shortcuts));

    window.dispatchEvent(new KeyboardEvent('keydown', { key: '?' }));
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'F5' }));

    expect(action1).toHaveBeenCalledTimes(1);
    expect(action2).toHaveBeenCalledTimes(1);
  });

  it('should handle Meta key as Ctrl', () => {
    const action = vi.fn();
    const shortcuts = { 'Ctrl+p': action };

    renderHook(() => useKeyboard(shortcuts));

    const event = new KeyboardEvent('keydown', { key: 'p', metaKey: true });
    window.dispatchEvent(event);

    expect(action).toHaveBeenCalledTimes(1);
  });
});
