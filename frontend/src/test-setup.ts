import '@testing-library/jest-dom';
import { afterEach } from 'vitest';
import { cleanup } from '@testing-library/react';

// Clean up DOM after each test to prevent cross-test pollution from
// library portals (e.g. sonner Toaster injects into document.body).
afterEach(() => {
  cleanup();
  document.body.querySelectorAll('[data-sonner-toaster]').forEach((el) => el.remove());
});

// Mock localStorage for zustand persist
const localStorageMock = {
  getItem: () => null,
  setItem: () => {},
  removeItem: () => {},
  clear: () => {},
  length: 0,
  key: () => null,
};
Object.defineProperty(window, 'localStorage', { value: localStorageMock });
