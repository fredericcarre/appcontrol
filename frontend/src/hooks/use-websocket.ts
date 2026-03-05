import { useEffect, useRef, useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useAuthStore } from '@/stores/auth';
import { useWebSocketStore } from '@/stores/websocket';

// Global WebSocket instance for sharing across components
let globalWs: WebSocket | null = null;

export function getGlobalWebSocket(): WebSocket | null {
  return globalWs;
}

export function useWebSocket() {
  const wsRef = useRef<WebSocket | null>(null);
  const token = useAuthStore((s) => s.token);
  const setConnected = useWebSocketStore((s) => s.setConnected);
  const addMessage = useWebSocketStore((s) => s.addMessage);
  const queryClient = useQueryClient();
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const reconnectDelay = useRef(1000);
  const connectRef = useRef<(() => void) | undefined>(undefined);
  const pingTimer = useRef<ReturnType<typeof setInterval> | undefined>(undefined);

  const connect = useCallback(() => {
    if (!token) return;
    if (wsRef.current?.readyState === WebSocket.OPEN) return;

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const ws = new WebSocket(`${protocol}//${window.location.host}/ws?token=${token}`);

    ws.onopen = () => {
      setConnected(true);
      reconnectDelay.current = 1000;
      const subs = useWebSocketStore.getState().subscribedApps;
      subs.forEach((appId) => {
        ws.send(JSON.stringify({ type: 'subscribe', payload: { app_id: appId } }));
      });

      // Send ping every 30 seconds to keep connection alive
      if (pingTimer.current) clearInterval(pingTimer.current);
      pingTimer.current = setInterval(() => {
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
      if (pingTimer.current) clearInterval(pingTimer.current);
      reconnectTimer.current = setTimeout(() => {
        reconnectDelay.current = Math.min(reconnectDelay.current * 2, 60000);
        connectRef.current?.();
      }, reconnectDelay.current);
    };

    ws.onerror = () => {
      ws.close();
    };

    wsRef.current = ws;
    globalWs = ws;
  }, [token, setConnected, addMessage, queryClient]);

  useEffect(() => {
    connectRef.current = connect;
  });

  const subscribe = useCallback((appId: string) => {
    useWebSocketStore.getState().addSubscription(appId);
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ type: 'subscribe', payload: { app_id: appId } }));
    }
  }, []);

  const unsubscribe = useCallback((appId: string) => {
    useWebSocketStore.getState().removeSubscription(appId);
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ type: 'unsubscribe', payload: { app_id: appId } }));
    }
  }, []);

  useEffect(() => {
    connect();
    return () => {
      clearTimeout(reconnectTimer.current);
      wsRef.current?.close();
    };
  }, [connect]);

  return { subscribe, unsubscribe };
}
