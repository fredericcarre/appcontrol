import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';

// Mock the teams API
vi.mock('@/api/teams', () => ({
  useTeams: vi.fn(),
}));

import { useTeams } from '@/api/teams';
const mockedUseTeams = vi.mocked(useTeams);

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('TeamList', () => {
  const onSelect = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should show loading spinner when data is loading', async () => {
    mockedUseTeams.mockReturnValue({
      data: undefined,
      isLoading: true,
    } as ReturnType<typeof useTeams>);

    const { TeamList } = await import('./TeamList');
    const { container } = render(
      React.createElement(TeamList, { onSelect }),
      { wrapper: createWrapper() },
    );

    const spinner = container.querySelector('.animate-spin');
    expect(spinner).toBeInTheDocument();
  });

  it('should render team names', async () => {
    mockedUseTeams.mockReturnValue({
      data: [
        { id: 't1', name: 'DevOps', description: 'DevOps team', org_id: 'o1', member_count: 5, created_at: '2024-01-01T00:00:00Z' },
        { id: 't2', name: 'Platform', description: 'Platform team', org_id: 'o1', member_count: 3, created_at: '2024-01-01T00:00:00Z' },
      ],
      isLoading: false,
    } as unknown as ReturnType<typeof useTeams>);

    const { TeamList } = await import('./TeamList');
    render(React.createElement(TeamList, { onSelect }), { wrapper: createWrapper() });

    expect(screen.getByText('DevOps')).toBeInTheDocument();
    expect(screen.getByText('Platform')).toBeInTheDocument();
  });

  it('should render team descriptions', async () => {
    mockedUseTeams.mockReturnValue({
      data: [
        { id: 't1', name: 'DevOps', description: 'DevOps engineers', org_id: 'o1', member_count: 5, created_at: '2024-01-01T00:00:00Z' },
      ],
      isLoading: false,
    } as unknown as ReturnType<typeof useTeams>);

    const { TeamList } = await import('./TeamList');
    render(React.createElement(TeamList, { onSelect }), { wrapper: createWrapper() });

    expect(screen.getByText('DevOps engineers')).toBeInTheDocument();
  });

  it('should display member count badge', async () => {
    mockedUseTeams.mockReturnValue({
      data: [
        { id: 't1', name: 'DevOps', description: 'Desc', org_id: 'o1', member_count: 7, created_at: '2024-01-01T00:00:00Z' },
      ],
      isLoading: false,
    } as unknown as ReturnType<typeof useTeams>);

    const { TeamList } = await import('./TeamList');
    render(React.createElement(TeamList, { onSelect }), { wrapper: createWrapper() });

    expect(screen.getByText('7')).toBeInTheDocument();
  });

  it('should call onSelect with team id when clicked', async () => {
    mockedUseTeams.mockReturnValue({
      data: [
        { id: 't1', name: 'DevOps', description: 'Desc', org_id: 'o1', member_count: 5, created_at: '2024-01-01T00:00:00Z' },
      ],
      isLoading: false,
    } as unknown as ReturnType<typeof useTeams>);

    const { TeamList } = await import('./TeamList');
    render(React.createElement(TeamList, { onSelect }), { wrapper: createWrapper() });

    fireEvent.click(screen.getByText('DevOps'));
    expect(onSelect).toHaveBeenCalledWith('t1');
  });

  it('should render multiple teams with correct click handlers', async () => {
    mockedUseTeams.mockReturnValue({
      data: [
        { id: 't1', name: 'DevOps', description: 'Desc', org_id: 'o1', member_count: 5, created_at: '2024-01-01T00:00:00Z' },
        { id: 't2', name: 'Platform', description: 'Desc', org_id: 'o1', member_count: 3, created_at: '2024-01-01T00:00:00Z' },
      ],
      isLoading: false,
    } as unknown as ReturnType<typeof useTeams>);

    const { TeamList } = await import('./TeamList');
    render(React.createElement(TeamList, { onSelect }), { wrapper: createWrapper() });

    fireEvent.click(screen.getByText('Platform'));
    expect(onSelect).toHaveBeenCalledWith('t2');
  });

  it('should render nothing extra when teams is empty', async () => {
    mockedUseTeams.mockReturnValue({
      data: [],
      isLoading: false,
    } as unknown as ReturnType<typeof useTeams>);

    const { TeamList } = await import('./TeamList');
    const { container } = render(
      React.createElement(TeamList, { onSelect }),
      { wrapper: createWrapper() },
    );

    // Should have no buttons
    expect(container.querySelectorAll('button')).toHaveLength(0);
  });
});
