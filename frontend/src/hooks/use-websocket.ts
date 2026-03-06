import { useEffect, useCallback, useRef } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useAuthStore } from '@/stores/auth';
import { useWebSocketStore } from '@/stores/websocket';

// Global WebSocket instance for sharing across components (singleton)
let globalWs: WebSocket | null = null;
let globalReconnectTimer: ReturnType<typeof setTimeout> | undefined = undefined;
let globalReconnectDelay = 1000;
let globalPingTimer: ReturnType<typeof setInterval> | undefined = undefined;
let globalConnectionCount = 0;

export function getGlobalWebSocket(): WebSocket | null {
  return globalWs;
}

export function useWebSocket() {
  const token = useAuthStore((s) => s.token);
  const setConnected = useWebSocketStore((s) => s.setConnected);
  const addMessage = useWebSocketStore((s) => s.addMessage);
  const queryClient = useQueryClient();
  const connectRef = useRef<(() => void) | undefined>(undefined);

  const connect = useCallback(() => {
    if (!token) return;
    // Use the global WebSocket - only create if not already open/connecting
    if (globalWs?.readyState === WebSocket.OPEN || globalWs?.readyState === WebSocket.CONNECTING) return;

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(`${protocol}//${window.location.host}/ws?token=${token}`);

    ws.onopen = () => {
      setConnected(true);
      globalReconnectDelay = 1000;
      const subs = useWebSocketStore.getState().subscribedApps;
      subs.forEach((appId) => {
        ws.send(JSON.stringify({ type: 'subscribe', payload: { app_id: appId } }));
      });

      // Send ping every 30 seconds to keep connection alive
      if (globalPingTimer) clearInterval(globalPingTimer);
      globalPingTimer = setInterval(() => {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ type: 'ping' }));
        }
      }, 30000);
    };

    ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data);
        addMessage({ ...msg, timestamp: msg.timestamp || new Date().toISOString() });

        // Update component state immediately in cache for responsive UI
        if (msg.type === 'StateChange' && msg.payload?.app_id && msg.payload?.component_id) {
          const appId = msg.payload.app_id;
          const componentId = msg.payload.component_id;
          const newState = msg.payload.to;

          // Update the app detail cache directly for instant feedback
          queryClient.setQueryData(['apps', appId], (oldData: unknown) => {
            if (!oldData || typeof oldData !== 'object') return oldData;
            const data = oldData as { components?: Array<{ id: string; current_state: string }> };
            if (!data.components) return oldData;

            return {
              ...data,
              components: data.components.map((c) =>
                c.id === componentId ? { ...c, current_state: newState } : c
              ),
            };
          });

          // Also invalidate to ensure we get full server state eventually
          queryClient.invalidateQueries({ queryKey: ['apps', appId] });
        }
      } catch {
        // ignore parse errors
      }
    };

    ws.onclose = () => {
      setConnected(false);
      globalWs = null;
      if (globalPingTimer) clearInterval(globalPingTimer);
      globalPingTimer = undefined;
      // Only reconnect if there are still active users of this hook
      if (globalConnectionCount > 0) {
        globalReconnectTimer = setTimeout(() => {
          globalReconnectDelay = Math.min(globalReconnectDelay * 2, 60000);
          connectRef.current?.();
        }, globalReconnectDelay);
      }
    };

    ws.onerror = () => {
      ws.close();
    };

    globalWs = ws;
  }, [token, setConnected, addMessage, queryClient]);

  useEffect(() => {
    connectRef.current = connect;
  });

  const subscribe = useCallback((appId: string) => {
    useWebSocketStore.getState().addSubscription(appId);
    if (globalWs?.readyState === WebSocket.OPEN) {
      globalWs.send(JSON.stringify({ type: 'subscribe', payload: { app_id: appId } }));
    }
  }, []);

  const unsubscribe = useCallback((appId: string) => {
    useWebSocketStore.getState().removeSubscription(appId);
    if (globalWs?.readyState === WebSocket.OPEN) {
      globalWs.send(JSON.stringify({ type: 'unsubscribe', payload: { app_id: appId } }));
    }
  }, []);

  useEffect(() => {
    globalConnectionCount++;
    connect();
    return () => {
      globalConnectionCount--;
      // Only close if no more users of this hook
      if (globalConnectionCount === 0) {
        clearTimeout(globalReconnectTimer);
        globalReconnectTimer = undefined;
        globalWs?.close();
        globalWs = null;
      }
    };
  }, [connect]);

  return { subscribe, unsubscribe };
}
