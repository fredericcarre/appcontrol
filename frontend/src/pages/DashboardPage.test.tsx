import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import { useWebSocketStore } from '@/stores/websocket';

// Mock the apps API
vi.mock('@/api/apps', () => ({
  useApps: vi.fn(),
  useStartApp: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
  useStopApp: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
  useCancelOperation: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
}));

import { useApps } from '@/api/apps';
const mockedUseApps = vi.mocked(useApps);

const mockNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual('react-router-dom');
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(
      QueryClientProvider,
      { client: queryClient },
      React.createElement(MemoryRouter, null, children),
    );
  };
}

describe('DashboardPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useWebSocketStore.setState({
      connected: true,
      messages: [],
      subscribedApps: new Set(),
    });
  });

  it('should show loading spinner when data is loading', async () => {
    mockedUseApps.mockReturnValue({
      data: undefined,
      isLoading: true,
    } as ReturnType<typeof useApps>);

    const { DashboardPage } = await import('./DashboardPage');
    render(React.createElement(DashboardPage), { wrapper: createWrapper() });

    // Should show a spinner
    const spinner = document.querySelector('.animate-spin');
    expect(spinner).toBeInTheDocument();
  });

  it('should show dashboard title when loaded', async () => {
    mockedUseApps.mockReturnValue({
      data: [],
      isLoading: false,
    } as unknown as ReturnType<typeof useApps>);

    const { DashboardPage } = await import('./DashboardPage');
    render(React.createElement(DashboardPage), { wrapper: createWrapper() });

    expect(screen.getByText('Dashboard')).toBeInTheDocument();
  });

  it('should show New Application button', async () => {
    mockedUseApps.mockReturnValue({
      data: [],
      isLoading: false,
    } as unknown as ReturnType<typeof useApps>);

    const { DashboardPage } = await import('./DashboardPage');
    render(React.createElement(DashboardPage), { wrapper: createWrapper() });

    expect(screen.getByText('New Application')).toBeInTheDocument();
  });

  it('should navigate to onboarding when New Application is clicked', async () => {
    mockedUseApps.mockReturnValue({
      data: [],
      isLoading: false,
    } as unknown as ReturnType<typeof useApps>);

    const { DashboardPage } = await import('./DashboardPage');
    render(React.createElement(DashboardPage), { wrapper: createWrapper() });

    fireEvent.click(screen.getByText('New Application'));
    expect(mockNavigate).toHaveBeenCalledWith('/onboarding');
  });

  it('should show empty state when no apps', async () => {
    mockedUseApps.mockReturnValue({
      data: [],
      isLoading: false,
    } as unknown as ReturnType<typeof useApps>);

    const { DashboardPage } = await import('./DashboardPage');
    render(React.createElement(DashboardPage), { wrapper: createWrapper() });

    expect(screen.getByText('No applications yet. Create one to get started.')).toBeInTheDocument();
  });

  it('should display stats cards with correct counts', async () => {
    const apps = [
      { id: '1', name: 'App 1', description: 'Desc', weather: 'sunny', component_count: 5 },
      { id: '2', name: 'App 2', description: 'Desc', weather: 'fair', component_count: 3 },
      { id: '3', name: 'App 3', description: 'Desc', weather: 'cloudy', component_count: 2 },
      { id: '4', name: 'App 4', description: 'Desc', weather: 'stormy', component_count: 1 },
    ];

    mockedUseApps.mockReturnValue({
      data: apps,
      isLoading: false,
    } as unknown as ReturnType<typeof useApps>);

    const { DashboardPage } = await import('./DashboardPage');
    render(React.createElement(DashboardPage), { wrapper: createWrapper() });

    // Verify stat labels exist
    expect(screen.getByText('Total Apps')).toBeInTheDocument();
    expect(screen.getByText('Running')).toBeInTheDocument();
    expect(screen.getByText('Degraded / Stopped')).toBeInTheDocument();
    expect(screen.getByText('Failed')).toBeInTheDocument();

    // Find the stat values next to their labels
    const totalLabel = screen.getByText('Total Apps');
    const totalValue = totalLabel.parentElement?.querySelector('p.text-2xl');
    expect(totalValue?.textContent).toBe('4');

    const runningLabel = screen.getByText('Running');
    const runningValue = runningLabel.parentElement?.querySelector('p.text-2xl');
    // Note: No apps have global_state='RUNNING' in the test data, so 0
    expect(runningValue?.textContent).toBe('0');

    const degradedLabel = screen.getByText('Degraded / Stopped');
    const degradedValue = degradedLabel.parentElement?.querySelector('p.text-2xl');
    // Note: No apps have global_state='DEGRADED' or 'STOPPED' in the test data
    expect(degradedValue?.textContent).toBe('0');

    const failedLabel = screen.getByText('Failed');
    const failedValue = failedLabel.parentElement?.querySelector('p.text-2xl');
    // Note: No apps have global_state='FAILED' in the test data
    expect(failedValue?.textContent).toBe('0');
  });

  it('should display application names in the list', async () => {
    const apps = [
      { id: '1', name: 'MyApp', description: 'My App Desc', weather: 'sunny', global_state: 'RUNNING', running_count: 5, stopped_count: 0 },
    ];

    mockedUseApps.mockReturnValue({
      data: apps,
      isLoading: false,
    } as unknown as ReturnType<typeof useApps>);

    const { DashboardPage } = await import('./DashboardPage');
    render(React.createElement(DashboardPage), { wrapper: createWrapper() });

    expect(screen.getByText('MyApp')).toBeInTheDocument();
    expect(screen.getByText('My App Desc')).toBeInTheDocument();
    expect(screen.getByText('5 running')).toBeInTheDocument();
  });

  it('should navigate to app when app is clicked', async () => {
    const apps = [
      { id: 'app-123', name: 'MyApp', description: 'Desc', weather: 'sunny', component_count: 5 },
    ];

    mockedUseApps.mockReturnValue({
      data: apps,
      isLoading: false,
    } as unknown as ReturnType<typeof useApps>);

    const { DashboardPage } = await import('./DashboardPage');
    render(React.createElement(DashboardPage), { wrapper: createWrapper() });

    fireEvent.click(screen.getByText('MyApp'));
    expect(mockNavigate).toHaveBeenCalledWith('/apps/app-123');
  });

  it('should show "No recent events" when no WebSocket messages', async () => {
    mockedUseApps.mockReturnValue({
      data: [],
      isLoading: false,
    } as unknown as ReturnType<typeof useApps>);

    const { DashboardPage } = await import('./DashboardPage');
    render(React.createElement(DashboardPage), { wrapper: createWrapper() });

    expect(screen.getByText('No recent events')).toBeInTheDocument();
  });

  it('should display recent WebSocket events', async () => {
    useWebSocketStore.setState({
      connected: true,
      messages: [
        { type: 'state_change', payload: {}, timestamp: '2024-01-01T10:00:00Z' },
        { type: 'check_result', payload: {}, timestamp: '2024-01-01T10:01:00Z' },
      ],
      subscribedApps: new Set(),
    });

    mockedUseApps.mockReturnValue({
      data: [],
      isLoading: false,
    } as unknown as ReturnType<typeof useApps>);

    const { DashboardPage } = await import('./DashboardPage');
    render(React.createElement(DashboardPage), { wrapper: createWrapper() });

    expect(screen.getByText('state_change')).toBeInTheDocument();
    expect(screen.getByText('check_result')).toBeInTheDocument();
  });

  it('should show global state badge for apps', async () => {
    const apps = [
      { id: '1', name: 'App 1', description: 'Desc', weather: 'sunny', global_state: 'RUNNING', component_count: 5 },
    ];

    mockedUseApps.mockReturnValue({
      data: apps,
      isLoading: false,
    } as unknown as ReturnType<typeof useApps>);

    const { DashboardPage } = await import('./DashboardPage');
    render(React.createElement(DashboardPage), { wrapper: createWrapper() });

    expect(screen.getByText('RUNNING')).toBeInTheDocument();
  });
});
