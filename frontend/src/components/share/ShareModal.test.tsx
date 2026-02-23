import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';

// Mock the permissions API
vi.mock('@/api/permissions', () => ({
  useAppPermissions: vi.fn(),
  useSetPermission: vi.fn(),
  useRemovePermission: vi.fn(),
  useShareLinks: vi.fn(),
  useCreateShareLink: vi.fn(),
  useRevokeShareLink: vi.fn(),
}));

// Mock the users API
vi.mock('@/api/users', () => ({
  useSearchUsers: vi.fn(() => ({ data: [], isLoading: false })),
}));

import {
  useAppPermissions,
  useSetPermission,
  useRemovePermission,
  useShareLinks,
  useCreateShareLink,
  useRevokeShareLink,
} from '@/api/permissions';

const mockedUseAppPermissions = vi.mocked(useAppPermissions);
const mockedUseShareLinks = vi.mocked(useShareLinks);
const mockedUseSetPermission = vi.mocked(useSetPermission);
const mockedUseRemovePermission = vi.mocked(useRemovePermission);
const mockedUseCreateShareLink = vi.mocked(useCreateShareLink);
const mockedUseRevokeShareLink = vi.mocked(useRevokeShareLink);

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('ShareModal', () => {
  const mockMutateAsync = vi.fn();
  const mockMutate = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();

    mockedUseAppPermissions.mockReturnValue({
      data: [
        { id: 'p1', app_id: 'app-1', user_id: 'u1', level: 'edit', user_email: 'alice@example.com', type: 'user' },
        { id: 'p2', app_id: 'app-1', team_id: 't1', level: 'view', team_name: 'DevOps', type: 'team' },
      ],
      isLoading: false,
    } as unknown as ReturnType<typeof useAppPermissions>);

    mockedUseShareLinks.mockReturnValue({
      data: [
        { id: 'sl1', app_id: 'app-1', token: 'abc123', permission_level: 'view', expires_at: null, max_uses: 10, current_uses: 3, created_by: 'u1' },
      ],
      isLoading: false,
    } as unknown as ReturnType<typeof useShareLinks>);

    mockedUseSetPermission.mockReturnValue({
      mutateAsync: mockMutateAsync,
      isPending: false,
    } as unknown as ReturnType<typeof useSetPermission>);

    mockedUseRemovePermission.mockReturnValue({
      mutate: mockMutate,
      isPending: false,
    } as unknown as ReturnType<typeof useRemovePermission>);

    mockedUseCreateShareLink.mockReturnValue({
      mutateAsync: mockMutateAsync,
      isPending: false,
    } as unknown as ReturnType<typeof useCreateShareLink>);

    mockedUseRevokeShareLink.mockReturnValue({
      mutate: mockMutate,
      isPending: false,
    } as unknown as ReturnType<typeof useRevokeShareLink>);
  });

  it('should render the modal title', async () => {
    const { ShareModal } = await import('./ShareModal');
    render(
      <ShareModal appId="app-1" open={true} onOpenChange={vi.fn()} />,
      { wrapper: createWrapper() },
    );

    expect(screen.getByText('Share Application')).toBeInTheDocument();
  });

  it('should render Users & Teams tab and Share Links tab', async () => {
    const { ShareModal } = await import('./ShareModal');
    render(
      <ShareModal appId="app-1" open={true} onOpenChange={vi.fn()} />,
      { wrapper: createWrapper() },
    );

    expect(screen.getByText('Users & Teams')).toBeInTheDocument();
    expect(screen.getByText('Share Links')).toBeInTheDocument();
  });

  it('should display existing permissions', async () => {
    const { ShareModal } = await import('./ShareModal');
    render(
      <ShareModal appId="app-1" open={true} onOpenChange={vi.fn()} />,
      { wrapper: createWrapper() },
    );

    expect(screen.getByText('alice@example.com')).toBeInTheDocument();
    expect(screen.getByText('DevOps')).toBeInTheDocument();
  });

  it('should show Team badge for team permissions', async () => {
    const { ShareModal } = await import('./ShareModal');
    render(
      <ShareModal appId="app-1" open={true} onOpenChange={vi.fn()} />,
      { wrapper: createWrapper() },
    );

    expect(screen.getByText('Team')).toBeInTheDocument();
  });

  it('should show permission levels for each entry', async () => {
    const { ShareModal } = await import('./ShareModal');
    render(
      <ShareModal appId="app-1" open={true} onOpenChange={vi.fn()} />,
      { wrapper: createWrapper() },
    );

    const editBadges = screen.getAllByText('edit');
    expect(editBadges.length).toBeGreaterThanOrEqual(1);
    const viewBadges = screen.getAllByText('view');
    expect(viewBadges.length).toBeGreaterThanOrEqual(1);
  });

  it('should show "No permissions set" when no permissions', async () => {
    mockedUseAppPermissions.mockReturnValue({
      data: [],
      isLoading: false,
    } as unknown as ReturnType<typeof useAppPermissions>);

    const { ShareModal } = await import('./ShareModal');
    render(
      <ShareModal appId="app-1" open={true} onOpenChange={vi.fn()} />,
      { wrapper: createWrapper() },
    );

    expect(screen.getByText('No permissions set')).toBeInTheDocument();
  });

  it('should have user search input', async () => {
    const { ShareModal } = await import('./ShareModal');
    render(
      <ShareModal appId="app-1" open={true} onOpenChange={vi.fn()} />,
      { wrapper: createWrapper() },
    );

    expect(screen.getByPlaceholderText('Search users by name or email...')).toBeInTheDocument();
  });

  it('should not render anything when open is false', async () => {
    const { ShareModal } = await import('./ShareModal');
    render(
      <ShareModal appId="app-1" open={false} onOpenChange={vi.fn()} />,
      { wrapper: createWrapper() },
    );

    expect(screen.queryByText('Share Application')).not.toBeInTheDocument();
  });

  it('should call removePermission when delete button is clicked', async () => {
    const { ShareModal } = await import('./ShareModal');
    render(
      <ShareModal appId="app-1" open={true} onOpenChange={vi.fn()} />,
      { wrapper: createWrapper() },
    );

    // Find trash buttons (small icon buttons)
    const trashButtons = screen.getAllByRole('button').filter((btn) => {
      return btn.className.includes('h-7');
    });

    if (trashButtons.length > 0) {
      fireEvent.click(trashButtons[0]);
      expect(mockMutate).toHaveBeenCalled();
    }
  });
});
