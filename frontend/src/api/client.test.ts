import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import axios from 'axios';
import { useAuthStore } from '@/stores/auth';

// vi.hoisted runs before vi.mock — use it to declare shared state
const { requestHandlers, responseHandlers } = vi.hoisted(() => ({
  requestHandlers: [] as Array<{ fulfilled: (config: Record<string, unknown>) => Record<string, unknown> }>,
  responseHandlers: [] as Array<{ fulfilled: (response: unknown) => unknown; rejected: (error: unknown) => unknown }>,
}));

vi.mock('axios', () => {
  const interceptors = {
    request: {
      use: vi.fn((fulfilled: (config: Record<string, unknown>) => Record<string, unknown>) => {
        requestHandlers.push({ fulfilled });
        return 0;
      }),
    },
    response: {
      use: vi.fn((fulfilled: (r: unknown) => unknown, rejected: (e: unknown) => unknown) => {
        responseHandlers.push({ fulfilled, rejected });
        return 0;
      }),
    },
  };

  const instance = {
    interceptors,
    get: vi.fn(),
    post: vi.fn(),
    put: vi.fn(),
    delete: vi.fn(),
    defaults: { headers: { common: {} } },
  };

  return {
    default: {
      create: vi.fn(() => instance),
    },
  };
});

// Import client to trigger interceptor registration
import '@/api/client';

describe('API client', () => {
  let requestInterceptor: (config: Record<string, unknown>) => Record<string, unknown>;
  let responseErrorInterceptor: (error: unknown) => unknown;

  beforeEach(() => {
    useAuthStore.setState({ token: null, user: null });

    // Use the interceptors captured during module load
    if (requestHandlers.length > 0) {
      requestInterceptor = requestHandlers[requestHandlers.length - 1].fulfilled;
    }
    if (responseHandlers.length > 0) {
      responseErrorInterceptor = responseHandlers[responseHandlers.length - 1].rejected;
    }
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should create axios instance with correct config', () => {
    expect(axios.create).toHaveBeenCalledWith({
      baseURL: '/api',
      headers: { 'Content-Type': 'application/json' },
      withCredentials: true,
    });
  });

  it('should add Authorization header when token exists', () => {
    useAuthStore.setState({ token: 'test-jwt-token', user: null });

    const config = { headers: {} as Record<string, string> };
    const result = requestInterceptor(config);

    expect((result.headers as Record<string, string>).Authorization).toBe('Bearer test-jwt-token');
  });

  it('should not add Authorization header when no token', () => {
    useAuthStore.setState({ token: null, user: null });

    const config = { headers: {} as Record<string, string> };
    const result = requestInterceptor(config);

    expect((result.headers as Record<string, string>).Authorization).toBeUndefined();
  });

  it('should logout and redirect on 401 response', async () => {
    const user = { id: '1', email: 'test@test.com', name: 'Test', org_id: 'org-1', role: 'admin' };
    useAuthStore.setState({ token: 'valid-token', user });

    // Mock window.location
    const originalHref = window.location.href;
    Object.defineProperty(window, 'location', {
      value: { href: originalHref },
      writable: true,
    });

    // Mock fetch so the logout endpoint call doesn't fail
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true }));

    const error = { response: { status: 401 } };

    await expect(responseErrorInterceptor(error)).rejects.toEqual(error);

    expect(useAuthStore.getState().token).toBeNull();
    expect(useAuthStore.getState().user).toBeNull();
    expect(window.location.href).toBe('/login');
  });

  it('should reject non-401 errors without logout', async () => {
    const user = { id: '1', email: 'test@test.com', name: 'Test', org_id: 'org-1', role: 'admin' };
    useAuthStore.setState({ token: 'valid-token', user });

    const error = { response: { status: 500 } };

    await expect(responseErrorInterceptor(error)).rejects.toEqual(error);

    // Token should still be present
    expect(useAuthStore.getState().token).toBe('valid-token');
  });

  it('should reject errors with no response (network error) without logout', async () => {
    const user = { id: '1', email: 'test@test.com', name: 'Test', org_id: 'org-1', role: 'admin' };
    useAuthStore.setState({ token: 'valid-token', user });

    const error = { message: 'Network Error' };

    await expect(responseErrorInterceptor(error)).rejects.toEqual(error);

    expect(useAuthStore.getState().token).toBe('valid-token');
  });
});
