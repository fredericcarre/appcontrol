import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { useWebSocketStore } from './websocket';

describe('useWebSocketStore', () => {
  beforeEach(() => {
    vi.useFakeTimers();
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
  });

  describe('connection state', () => {
    it('should start disconnected', () => {
      expect(useWebSocketStore.getState().connected).toBe(false);
    });

    it('should set connected to true', () => {
      useWebSocketStore.getState().setConnected(true);
      expect(useWebSocketStore.getState().connected).toBe(true);
    });

    it('should set connected to false', () => {
      useWebSocketStore.getState().setConnected(true);
      useWebSocketStore.getState().setConnected(false);
      // rawConnected updates immediately, connected is debounced by 3 seconds
      expect(useWebSocketStore.getState().rawConnected).toBe(false);
      vi.advanceTimersByTime(3000);
      expect(useWebSocketStore.getState().connected).toBe(false);
    });
  });

  describe('messages', () => {
    it('should start with empty messages', () => {
      expect(useWebSocketStore.getState().messages).toEqual([]);
    });

    it('should add a message', () => {
      const msg = { type: 'state_change', payload: { component_id: 'c1' }, timestamp: '2024-01-01T00:00:00Z' };
      useWebSocketStore.getState().addMessage(msg);

      expect(useWebSocketStore.getState().messages).toHaveLength(1);
      expect(useWebSocketStore.getState().messages[0]).toEqual(msg);
    });

    it('should add multiple messages', () => {
      const msg1 = { type: 'state_change', payload: { id: '1' }, timestamp: '2024-01-01T00:00:00Z' };
      const msg2 = { type: 'check_result', payload: { id: '2' }, timestamp: '2024-01-01T00:00:01Z' };

      useWebSocketStore.getState().addMessage(msg1);
      useWebSocketStore.getState().addMessage(msg2);

      expect(useWebSocketStore.getState().messages).toHaveLength(2);
      expect(useWebSocketStore.getState().messages[0]).toEqual(msg1);
      expect(useWebSocketStore.getState().messages[1]).toEqual(msg2);
    });

    it('should limit messages to 1000 (keeps last 999 + new)', () => {
      // Add 1005 messages
      for (let i = 0; i < 1005; i++) {
        useWebSocketStore.getState().addMessage({
          type: 'event',
          payload: { index: i },
          timestamp: `2024-01-01T00:00:${String(i).padStart(2, '0')}Z`,
        });
      }

      expect(useWebSocketStore.getState().messages).toHaveLength(1000);
      // The first 5 messages should have been dropped
      expect(useWebSocketStore.getState().messages[0].payload).toEqual({ index: 5 });
      expect(useWebSocketStore.getState().messages[999].payload).toEqual({ index: 1004 });
    });

    it('should clear messages', () => {
      useWebSocketStore.getState().addMessage({
        type: 'event',
        payload: {},
        timestamp: '2024-01-01T00:00:00Z',
      });
      expect(useWebSocketStore.getState().messages).toHaveLength(1);

      useWebSocketStore.getState().clearMessages();
      expect(useWebSocketStore.getState().messages).toEqual([]);
    });
  });

  describe('subscriptions', () => {
    it('should start with no subscriptions', () => {
      expect(useWebSocketStore.getState().subscribedApps.size).toBe(0);
    });

    it('should add a subscription', () => {
      useWebSocketStore.getState().addSubscription('app-1');
      expect(useWebSocketStore.getState().subscribedApps.has('app-1')).toBe(true);
      expect(useWebSocketStore.getState().subscribedApps.size).toBe(1);
    });

    it('should add multiple subscriptions', () => {
      useWebSocketStore.getState().addSubscription('app-1');
      useWebSocketStore.getState().addSubscription('app-2');
      expect(useWebSocketStore.getState().subscribedApps.size).toBe(2);
      expect(useWebSocketStore.getState().subscribedApps.has('app-1')).toBe(true);
      expect(useWebSocketStore.getState().subscribedApps.has('app-2')).toBe(true);
    });

    it('should not duplicate subscriptions', () => {
      useWebSocketStore.getState().addSubscription('app-1');
      useWebSocketStore.getState().addSubscription('app-1');
      expect(useWebSocketStore.getState().subscribedApps.size).toBe(1);
    });

    it('should remove a subscription', () => {
      useWebSocketStore.getState().addSubscription('app-1');
      useWebSocketStore.getState().addSubscription('app-2');
      useWebSocketStore.getState().removeSubscription('app-1');

      expect(useWebSocketStore.getState().subscribedApps.has('app-1')).toBe(false);
      expect(useWebSocketStore.getState().subscribedApps.has('app-2')).toBe(true);
      expect(useWebSocketStore.getState().subscribedApps.size).toBe(1);
    });

    it('should handle removing non-existent subscription gracefully', () => {
      useWebSocketStore.getState().removeSubscription('non-existent');
      expect(useWebSocketStore.getState().subscribedApps.size).toBe(0);
    });
  });
});
