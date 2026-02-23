import { create } from 'zustand';
import { persist } from 'zustand/middleware';

interface User {
  id: string;
  email: string;
  name: string;
  org_id: string;
  role: string;
}

interface AuthState {
  // Token is kept in memory for backward compat (API key / CLI flows).
  // Browser auth uses HttpOnly cookies — the token field may be null
  // even when the user is authenticated (cookie is sent automatically).
  token: string | null;
  user: User | null;
  setAuth: (token: string | null, user: User) => void;
  logout: () => void;
  isAuthenticated: () => boolean;
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      token: null,
      user: null,
      setAuth: (token, user) => set({ token, user }),
      logout: () => {
        set({ token: null, user: null });
        // Call the logout endpoint to clear the HttpOnly cookie
        fetch('/api/v1/auth/logout', { method: 'POST', credentials: 'include' }).catch(() => {});
      },
      isAuthenticated: () => get().user !== null,
    }),
    {
      name: 'appcontrol-auth',
      // Only persist user info, not the token (token is in HttpOnly cookie)
      partialize: (state) => ({ user: state.user }),
    },
  ),
);
