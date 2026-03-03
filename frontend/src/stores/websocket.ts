import { create } from 'zustand';

export interface WsMessage {
  type: string;
  payload: Record<string, unknown>;
  timestamp: string;
}

interface WebSocketState {
  /** Actual connection state (raw) */
  rawConnected: boolean;
  /** Debounced connection state shown to users (delays offline by 3s) */
  connected: boolean;
  setConnected: (c: boolean) => void;
  messages: WsMessage[];
  addMessage: (msg: WsMessage) => void;
  clearMessages: () => void;
  subscribedApps: Set<string>;
  addSubscription: (appId: string) => void;
  removeSubscription: (appId: string) => void;
  /** Internal timer for debouncing offline state */
  _offlineTimer: ReturnType<typeof setTimeout> | null;
}

export const useWebSocketStore = create<WebSocketState>()((set, get) => ({
  rawConnected: false,
  connected: false,
  setConnected: (c) => {
    // Clear any pending offline timer
    const timer = get()._offlineTimer;
    if (timer) {
      clearTimeout(timer);
      set({ _offlineTimer: null });
    }

    if (c) {
      // Connected: update immediately
      set({ rawConnected: true, connected: true });
    } else {
      // Disconnected: update raw immediately, but delay UI update by 3 seconds
      // This prevents brief network blips from showing "Offline"
      set({ rawConnected: false });
      const offlineTimer = setTimeout(() => {
        // Only set offline if still disconnected
        if (!get().rawConnected) {
          set({ connected: false });
        }
        set({ _offlineTimer: null });
      }, 3000);
      set({ _offlineTimer: offlineTimer });
    }
  },
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
  _offlineTimer: null,
}));
