import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useAuthStore } from '@/stores/auth';
import { useWebSocketStore } from '@/stores/websocket';

// Mock WebSocket
class MockWebSocket {
  static OPEN = 1;
  static CLOSED = 3;
  static CONNECTING = 0;

  url: string;
  readyState: number = MockWebSocket.CONNECTING;
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  send = vi.fn();
  close = vi.fn();

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  static instances: MockWebSocket[] = [];
  static clear() {
    MockWebSocket.instances = [];
  }

  simulateOpen() {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.();
  }

  simulateMessage(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) });
  }

  simulateClose() {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.();
  }

  simulateError() {
    this.onerror?.();
  }
}

// Set up global WebSocket mock
const originalWebSocket = globalThis.WebSocket;

describe('useWebSocket', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    MockWebSocket.clear();
    globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;
    (globalThis.WebSocket as unknown as Record<string, number>).OPEN = MockWebSocket.OPEN;
    (globalThis.WebSocket as unknown as Record<string, number>).CLOSED = MockWebSocket.CLOSED;
    (globalThis.WebSocket as unknown as Record<string, number>).CONNECTING = MockWebSocket.CONNECTING;

    useAuthStore.setState({ token: 'test-token', user: { id: '1', email: 'test@test.com', name: 'Test', org_id: 'org-1', role: 'admin' } });
    useWebSocketStore.setState({
      rawConnected: false,
      connected: false,
      messages: [],
      subscribedApps: new Set<string>(),
      _offlineTimer: null,
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    globalThis.WebSocket = originalWebSocket;
    vi.restoreAllMocks();
  });

  it('should connect when token is available', async () => {
    const { useWebSocket } = await import('./use-websocket');

    renderHook(() => useWebSocket());

    expect(MockWebSocket.instances).toHaveLength(1);
    expect(MockWebSocket.instances[0].url).toContain('token=test-token');
  });

  it('should not connect when no token', async () => {
    useAuthStore.setState({ token: null, user: null });

    const { useWebSocket } = await import('./use-websocket');

    renderHook(() => useWebSocket());

    expect(MockWebSocket.instances).toHaveLength(0);
  });

  it('should set connected=true on open', async () => {
    const { useWebSocket } = await import('./use-websocket');

    renderHook(() => useWebSocket());

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
    });

    expect(useWebSocketStore.getState().connected).toBe(true);
  });

  it('should set connected=false on close', async () => {
    const { useWebSocket } = await import('./use-websocket');

    renderHook(() => useWebSocket());

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
    });

    expect(useWebSocketStore.getState().connected).toBe(true);

    act(() => {
      MockWebSocket.instances[0].simulateClose();
    });

    // rawConnected updates immediately, connected is debounced by 3 seconds
    expect(useWebSocketStore.getState().rawConnected).toBe(false);

    act(() => {
      vi.advanceTimersByTime(3000);
    });

    expect(useWebSocketStore.getState().connected).toBe(false);
  });

  it('should add messages from WebSocket', async () => {
    const { useWebSocket } = await import('./use-websocket');

    renderHook(() => useWebSocket());

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
    });

    act(() => {
      MockWebSocket.instances[0].simulateMessage({
        type: 'state_change',
        payload: { component_id: 'c1', state: 'RUNNING' },
        timestamp: '2024-01-01T00:00:00Z',
      });
    });

    const messages = useWebSocketStore.getState().messages;
    expect(messages).toHaveLength(1);
    expect(messages[0].type).toBe('state_change');
  });

  it('should re-subscribe to apps on open', async () => {
    useWebSocketStore.getState().addSubscription('app-1');
    useWebSocketStore.getState().addSubscription('app-2');

    const { useWebSocket } = await import('./use-websocket');

    renderHook(() => useWebSocket());

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
    });

    expect(MockWebSocket.instances[0].send).toHaveBeenCalledTimes(2);
    expect(MockWebSocket.instances[0].send).toHaveBeenCalledWith(
      JSON.stringify({ type: 'subscribe', payload: { app_id: 'app-1' } }),
    );
    expect(MockWebSocket.instances[0].send).toHaveBeenCalledWith(
      JSON.stringify({ type: 'subscribe', payload: { app_id: 'app-2' } }),
    );
  });

  it('should provide subscribe function', async () => {
    const { useWebSocket } = await import('./use-websocket');

    const { result } = renderHook(() => useWebSocket());

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
    });

    act(() => {
      result.current.subscribe('app-3');
    });

    expect(useWebSocketStore.getState().subscribedApps.has('app-3')).toBe(true);
    expect(MockWebSocket.instances[0].send).toHaveBeenCalledWith(
      JSON.stringify({ type: 'subscribe', payload: { app_id: 'app-3' } }),
    );
  });

  it('should provide unsubscribe function', async () => {
    const { useWebSocket } = await import('./use-websocket');

    const { result } = renderHook(() => useWebSocket());

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
    });

    act(() => {
      result.current.subscribe('app-3');
    });

    act(() => {
      result.current.unsubscribe('app-3');
    });

    expect(useWebSocketStore.getState().subscribedApps.has('app-3')).toBe(false);
    expect(MockWebSocket.instances[0].send).toHaveBeenCalledWith(
      JSON.stringify({ type: 'unsubscribe', payload: { app_id: 'app-3' } }),
    );
  });

  it('should ignore invalid JSON messages', async () => {
    const { useWebSocket } = await import('./use-websocket');

    renderHook(() => useWebSocket());

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
    });

    act(() => {
      MockWebSocket.instances[0].onmessage?.({ data: 'not valid json{' });
    });

    expect(useWebSocketStore.getState().messages).toHaveLength(0);
  });

  it('should close WebSocket on error', async () => {
    const { useWebSocket } = await import('./use-websocket');

    renderHook(() => useWebSocket());

    act(() => {
      MockWebSocket.instances[0].simulateError();
    });

    expect(MockWebSocket.instances[0].close).toHaveBeenCalled();
  });

  it('should clean up on unmount', async () => {
    const { useWebSocket } = await import('./use-websocket');

    const { unmount } = renderHook(() => useWebSocket());

    act(() => {
      MockWebSocket.instances[0].simulateOpen();
    });

    unmount();

    expect(MockWebSocket.instances[0].close).toHaveBeenCalled();
  });
});
