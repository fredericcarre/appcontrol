import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export interface SupervisionSettings {
  // Selected app IDs for slideshow (empty = all apps)
  selectedAppIds: string[];
  // Rotation interval in seconds
  intervalSeconds: number;
  // Whether to show app details panel
  showDetails: boolean;
  // Whether slideshow is currently running
  isPlaying: boolean;
  // Current app index in rotation
  currentIndex: number;
}

interface SupervisionStore extends SupervisionSettings {
  // Actions
  setSelectedAppIds: (ids: string[]) => void;
  toggleAppSelection: (appId: string) => void;
  selectAllApps: () => void;
  clearSelection: () => void;
  setIntervalSeconds: (seconds: number) => void;
  setShowDetails: (show: boolean) => void;
  play: () => void;
  pause: () => void;
  togglePlay: () => void;
  next: () => void;
  previous: () => void;
  goToIndex: (index: number) => void;
  reset: () => void;
}

const DEFAULT_SETTINGS: SupervisionSettings = {
  selectedAppIds: [],
  intervalSeconds: 30,
  showDetails: false,
  isPlaying: false,
  currentIndex: 0,
};

export const useSupervisionStore = create<SupervisionStore>()(
  persist(
    (set) => ({
      ...DEFAULT_SETTINGS,

      setSelectedAppIds: (ids) => set({ selectedAppIds: ids, currentIndex: 0 }),

      toggleAppSelection: (appId) =>
        set((state) => {
          const newIds = state.selectedAppIds.includes(appId)
            ? state.selectedAppIds.filter((id) => id !== appId)
            : [...state.selectedAppIds, appId];
          return { selectedAppIds: newIds, currentIndex: 0 };
        }),

      selectAllApps: () => set({ selectedAppIds: [], currentIndex: 0 }),

      clearSelection: () => set({ selectedAppIds: [], currentIndex: 0 }),

      setIntervalSeconds: (seconds) =>
        set({ intervalSeconds: Math.max(5, Math.min(300, seconds)) }),

      setShowDetails: (show) => set({ showDetails: show }),

      play: () => set({ isPlaying: true }),

      pause: () => set({ isPlaying: false }),

      togglePlay: () => set((state) => ({ isPlaying: !state.isPlaying })),

      next: () =>
        set((state) => ({
          currentIndex: state.currentIndex + 1,
        })),

      previous: () =>
        set((state) => ({
          currentIndex: Math.max(0, state.currentIndex - 1),
        })),

      goToIndex: (index) => set({ currentIndex: index }),

      reset: () => set({ ...DEFAULT_SETTINGS }),
    }),
    {
      name: 'appcontrol-supervision',
      partialize: (state) => ({
        selectedAppIds: state.selectedAppIds,
        intervalSeconds: state.intervalSeconds,
        showDetails: state.showDetails,
      }),
    }
  )
);
