import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useAuthStore } from '@/stores/auth';
import { useUiStore } from '@/stores/ui';
import { useWebSocketStore } from '@/stores/websocket';
import React from 'react';

// Mock all page components to avoid their internal dependencies
vi.mock('@/pages/DashboardPage', () => ({
  DashboardPage: () => React.createElement('div', { 'data-testid': 'dashboard-page' }, 'Dashboard Page'),
}));
vi.mock('@/pages/MapViewPage', () => ({
  MapViewPage: () => React.createElement('div', { 'data-testid': 'map-view-page' }, 'Map View Page'),
}));
vi.mock('@/pages/TeamsPage', () => ({
  TeamsPage: () => React.createElement('div', { 'data-testid': 'teams-page' }, 'Teams Page'),
}));
vi.mock('@/pages/AgentsPage', () => ({
  AgentsPage: () => React.createElement('div', { 'data-testid': 'agents-page' }, 'Agents Page'),
}));
vi.mock('@/pages/ReportsPage', () => ({
  ReportsPage: () => React.createElement('div', { 'data-testid': 'reports-page' }, 'Reports Page'),
}));
vi.mock('@/pages/SettingsPage', () => ({
  SettingsPage: () => React.createElement('div', { 'data-testid': 'settings-page' }, 'Settings Page'),
}));
vi.mock('@/pages/OnboardingPage', () => ({
  OnboardingPage: () => React.createElement('div', { 'data-testid': 'onboarding-page' }, 'Onboarding Page'),
}));
vi.mock('@/pages/ImportPage', () => ({
  default: () => React.createElement('div', { 'data-testid': 'import-page' }, 'Import Page'),
}));
vi.mock('@/pages/LoginPage', () => ({
  LoginPage: () => React.createElement('div', { 'data-testid': 'login-page' }, 'Login Page'),
}));

// Mock layout components to simplify rendering
vi.mock('@/components/layout/Sidebar', () => ({
  Sidebar: () => React.createElement('div', { 'data-testid': 'sidebar' }, 'Sidebar'),
}));
vi.mock('@/components/layout/Header', () => ({
  Header: () => React.createElement('div', { 'data-testid': 'header' }, 'Header'),
}));

function createWrapper(initialRoute: string) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(
      QueryClientProvider,
      { client: queryClient },
      React.createElement(MemoryRouter, { initialEntries: [initialRoute] }, children),
    );
  };
}

describe('App', () => {
  beforeEach(() => {
    useUiStore.setState({ sidebarCollapsed: false, theme: 'light', commandPaletteOpen: false });
    useWebSocketStore.setState({ connected: false, messages: [], subscribedApps: new Set() });
  });

  describe('when not authenticated', () => {
    beforeEach(() => {
      useAuthStore.setState({ token: null, user: null });
    });

    it('should render login page on /login', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/login') });

      expect(screen.getByTestId('login-page')).toBeInTheDocument();
    });

    it('should redirect to /login from root', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/') });

      expect(screen.getByTestId('login-page')).toBeInTheDocument();
    });

    it('should redirect to /login from any route', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/teams') });

      expect(screen.getByTestId('login-page')).toBeInTheDocument();
    });
  });

  describe('when authenticated', () => {
    beforeEach(() => {
      useAuthStore.setState({
        token: 'valid-jwt-token',
        user: { id: '1', email: 'admin@test.com', name: 'Admin', org_id: 'org-1', role: 'admin' },
      });
    });

    it('should render dashboard on root route', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/') });

      expect(screen.getByTestId('dashboard-page')).toBeInTheDocument();
    });

    it('should render sidebar and header', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/') });

      expect(screen.getByTestId('sidebar')).toBeInTheDocument();
      expect(screen.getByTestId('header')).toBeInTheDocument();
    });

    it('should render teams page on /teams', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/teams') });

      expect(screen.getByTestId('teams-page')).toBeInTheDocument();
    });

    it('should render agents page on /agents', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/agents') });

      expect(screen.getByTestId('agents-page')).toBeInTheDocument();
    });

    it('should render reports page on /reports', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/reports') });

      expect(screen.getByTestId('reports-page')).toBeInTheDocument();
    });

    it('should render settings page on /settings', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/settings') });

      expect(screen.getByTestId('settings-page')).toBeInTheDocument();
    });

    it('should render onboarding page on /onboarding', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/onboarding') });

      expect(screen.getByTestId('onboarding-page')).toBeInTheDocument();
    });

    it('should render import page on /import', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/import') });

      expect(screen.getByTestId('import-page')).toBeInTheDocument();
    });

    it('should redirect /login to / when authenticated', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/login') });

      expect(screen.getByTestId('dashboard-page')).toBeInTheDocument();
    });

    it('should redirect unknown routes to /', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/nonexistent') });

      expect(screen.getByTestId('dashboard-page')).toBeInTheDocument();
    });

    it('should render map view page on /apps/:appId', async () => {
      const App = (await import('./App')).default;
      render(React.createElement(App), { wrapper: createWrapper('/apps/app-123') });

      expect(screen.getByTestId('map-view-page')).toBeInTheDocument();
    });
  });
});
