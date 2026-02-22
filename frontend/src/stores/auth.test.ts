import { describe, it, expect, beforeEach } from 'vitest';
import { useAuthStore } from './auth';

describe('useAuthStore', () => {
  beforeEach(() => {
    useAuthStore.setState({ token: null, user: null });
  });

  it('should start with no auth', () => {
    const state = useAuthStore.getState();
    expect(state.token).toBeNull();
    expect(state.user).toBeNull();
  });

  it('should set auth token and user', () => {
    const user = { id: '1', email: 'test@example.com', name: 'Test', org_id: 'org-1', role: 'admin' };
    useAuthStore.getState().setAuth('jwt-token-123', user);

    const state = useAuthStore.getState();
    expect(state.token).toBe('jwt-token-123');
    expect(state.user).toEqual(user);
  });

  it('should clear auth on logout', () => {
    const user = { id: '1', email: 'test@example.com', name: 'Test', org_id: 'org-1', role: 'user' };
    useAuthStore.getState().setAuth('jwt-token', user);
    useAuthStore.getState().logout();

    const state = useAuthStore.getState();
    expect(state.token).toBeNull();
    expect(state.user).toBeNull();
  });
});
