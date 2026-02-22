import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import axios from 'axios';
import { useAuthStore } from '@/stores/auth';

// We need to test the client module, but since it creates an axios instance at
// module load time, we test the interceptor behaviors by importing the module fresh.

// Mock axios
vi.mock('axios', () => {
  const interceptors = {
    request: { use: vi.fn(), handlers: [] as Array<{ fulfilled: (config: Record<string, unknown>) => Record<string, unknown> }> },
    response: { use: vi.fn(), handlers: [] as Array<{ fulfilled: (response: unknown) => unknown; rejected: (error: unknown) => unknown }> },
  };

  // Capture interceptor callbacks when use() is called
  interceptors.request.use = vi.fn((fulfilled) => {
    interceptors.request.handlers.push({ fulfilled });
    return 0;
  }) as unknown as typeof interceptors.request.use;

  interceptors.response.use = vi.fn((fulfilled, rejected) => {
    interceptors.response.handlers.push({ fulfilled, rejected });
    return 0;
  }) as unknown as typeof interceptors.response.use;

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

describe('API client', () => {
  let requestInterceptor: (config: Record<string, unknown>) => Record<string, unknown>;
  let responseErrorInterceptor: (error: unknown) => unknown;

  beforeEach(async () => {
    vi.resetModules();
    useAuthStore.setState({ token: null, user: null });

    // Re-import to trigger interceptor registration
    await import('@/api/client');

    const mockAxios = axios as unknown as { create: ReturnType<typeof vi.fn> };
    const instance = mockAxios.create.mock.results[mockAxios.create.mock.results.length - 1]?.value;

    if (instance) {
      const reqHandlers = instance.interceptors.request.handlers;
      const resHandlers = instance.interceptors.response.handlers;
      if (reqHandlers.length > 0) {
        requestInterceptor = reqHandlers[reqHandlers.length - 1].fulfilled;
      }
      if (resHandlers.length > 0) {
        responseErrorInterceptor = resHandlers[resHandlers.length - 1].rejected;
      }
    }
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should create axios instance with correct baseURL and headers', () => {
    expect(axios.create).toHaveBeenCalledWith({
      baseURL: '/api',
      headers: { 'Content-Type': 'application/json' },
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
