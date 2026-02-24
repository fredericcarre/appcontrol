import { create } from 'zustand';

export interface WsMessage {
  type: string;
  payload: Record<string, unknown>;
  timestamp: string;
}

interface WebSocketState {
  connected: boolean;
  setConnected: (c: boolean) => void;
  messages: WsMessage[];
  addMessage: (msg: WsMessage) => void;
  clearMessages: () => void;
  subscribedApps: Set<string>;
  addSubscription: (appId: string) => void;
  removeSubscription: (appId: string) => void;
}

export const useWebSocketStore = create<WebSocketState>()((set, get) => ({
  connected: false,
  setConnected: (c) => set({ connected: c }),
  messages: [],
  addMessage: (msg) =>
    set((s) => ({
      messages: [...s.messages.slice(-999), msg],
    })),
  clearMessages: () => set({ messages: [] }),
  subscribedApps: new Set<string>(),
  addSubscription: (appId) => {
    const next = new Set(get().subscribedApps);
    next.add(appId);
    set({ subscribedApps: next });
  },
  removeSubscription: (appId) => {
    const next = new Set(get().subscribedApps);
    next.delete(appId);
    set({ subscribedApps: next });
  },
}));
