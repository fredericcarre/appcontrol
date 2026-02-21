import { useEffect, useRef, useCallback } from 'react';
import { useAuthStore } from '@/stores/auth';
import { useWebSocketStore } from '@/stores/websocket';

export function useWebSocket() {
  const wsRef = useRef<WebSocket | null>(null);
  const token = useAuthStore((s) => s.token);
  const setConnected = useWebSocketStore((s) => s.setConnected);
  const addMessage = useWebSocketStore((s) => s.addMessage);
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const reconnectDelay = useRef(1000);
  const connectRef = useRef<() => void>();

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
    };

    ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data);
        addMessage({ ...msg, timestamp: msg.timestamp || new Date().toISOString() });
      } catch {
        // ignore parse errors
      }
    };

    ws.onclose = () => {
      setConnected(false);
      reconnectTimer.current = setTimeout(() => {
        reconnectDelay.current = Math.min(reconnectDelay.current * 2, 60000);
        connectRef.current?.();
      }, reconnectDelay.current);
    };

    ws.onerror = () => {
      ws.close();
    };

    wsRef.current = ws;
  }, [token, setConnected, addMessage]);

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
