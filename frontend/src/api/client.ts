import axios from 'axios';
import { useAuthStore } from '@/stores/auth';

const client = axios.create({
  baseURL: '/api/v1',
  headers: { 'Content-Type': 'application/json' },
  // Send HttpOnly cookies automatically with every request.
  // The JWT is stored in an HttpOnly cookie set by the backend
  // (not in localStorage, which is vulnerable to XSS).
  withCredentials: true,
});

client.interceptors.request.use((config) => {
  // For backward compatibility: if a token is stored in memory (API key flow,
  // CLI integration), send it as a Bearer header. Browser auth uses cookies.
  const token = useAuthStore.getState().token;
  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

client.interceptors.response.use(
  (response) => response,
  (error) => {
    // Don't redirect on 401 for auth endpoints (login returns 401 for invalid credentials)
    const isAuthEndpoint = error.config?.url?.startsWith('/auth/');
    if (error.response?.status === 401 && !isAuthEndpoint) {
      useAuthStore.getState().logout();
      window.location.href = '/login';
    }
    return Promise.reject(error);
  },
);

export default client;
