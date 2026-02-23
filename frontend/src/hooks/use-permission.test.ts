import { describe, it, expect, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';

// Mock the API module
vi.mock('@/api/permissions', () => ({
  useEffectivePermission: vi.fn(),
}));

import { useEffectivePermission } from '@/api/permissions';
import { usePermission } from './use-permission';

const mockedUseEffectivePermission = vi.mocked(useEffectivePermission);

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('usePermission', () => {
  it('should return "none" level when no data is loaded', () => {
    mockedUseEffectivePermission.mockReturnValue({
      data: undefined,
      isLoading: true,
    } as ReturnType<typeof useEffectivePermission>);

    const { result } = renderHook(() => usePermission('app-1'), {
      wrapper: createWrapper(),
    });

    expect(result.current.level).toBe('none');
    expect(result.current.isLoading).toBe(true);
    expect(result.current.canView).toBe(false);
    expect(result.current.canOperate).toBe(false);
    expect(result.current.canEdit).toBe(false);
    expect(result.current.canManage).toBe(false);
    expect(result.current.isOwner).toBe(false);
  });

  it('should return correct permissions for view level', () => {
    mockedUseEffectivePermission.mockReturnValue({
      data: 'view',
      isLoading: false,
    } as ReturnType<typeof useEffectivePermission>);

    const { result } = renderHook(() => usePermission('app-1'), {
      wrapper: createWrapper(),
    });

    expect(result.current.level).toBe('view');
    expect(result.current.isLoading).toBe(false);
    expect(result.current.canView).toBe(true);
    expect(result.current.canOperate).toBe(false);
    expect(result.current.canEdit).toBe(false);
    expect(result.current.canManage).toBe(false);
    expect(result.current.isOwner).toBe(false);
  });

  it('should return correct permissions for operate level', () => {
    mockedUseEffectivePermission.mockReturnValue({
      data: 'operate',
      isLoading: false,
    } as ReturnType<typeof useEffectivePermission>);

    const { result } = renderHook(() => usePermission('app-1'), {
      wrapper: createWrapper(),
    });

    expect(result.current.level).toBe('operate');
    expect(result.current.canView).toBe(true);
    expect(result.current.canOperate).toBe(true);
    expect(result.current.canEdit).toBe(false);
    expect(result.current.canManage).toBe(false);
    expect(result.current.isOwner).toBe(false);
  });

  it('should return correct permissions for edit level', () => {
    mockedUseEffectivePermission.mockReturnValue({
      data: 'edit',
      isLoading: false,
    } as ReturnType<typeof useEffectivePermission>);

    const { result } = renderHook(() => usePermission('app-1'), {
      wrapper: createWrapper(),
    });

    expect(result.current.level).toBe('edit');
    expect(result.current.canView).toBe(true);
    expect(result.current.canOperate).toBe(true);
    expect(result.current.canEdit).toBe(true);
    expect(result.current.canManage).toBe(false);
    expect(result.current.isOwner).toBe(false);
  });

  it('should return correct permissions for manage level', () => {
    mockedUseEffectivePermission.mockReturnValue({
      data: 'manage',
      isLoading: false,
    } as ReturnType<typeof useEffectivePermission>);

    const { result } = renderHook(() => usePermission('app-1'), {
      wrapper: createWrapper(),
    });

    expect(result.current.level).toBe('manage');
    expect(result.current.canView).toBe(true);
    expect(result.current.canOperate).toBe(true);
    expect(result.current.canEdit).toBe(true);
    expect(result.current.canManage).toBe(true);
    expect(result.current.isOwner).toBe(false);
  });

  it('should return correct permissions for owner level', () => {
    mockedUseEffectivePermission.mockReturnValue({
      data: 'owner',
      isLoading: false,
    } as ReturnType<typeof useEffectivePermission>);

    const { result } = renderHook(() => usePermission('app-1'), {
      wrapper: createWrapper(),
    });

    expect(result.current.level).toBe('owner');
    expect(result.current.canView).toBe(true);
    expect(result.current.canOperate).toBe(true);
    expect(result.current.canEdit).toBe(true);
    expect(result.current.canManage).toBe(true);
    expect(result.current.isOwner).toBe(true);
  });

  it('should provide a can() function that checks arbitrary levels', () => {
    mockedUseEffectivePermission.mockReturnValue({
      data: 'edit',
      isLoading: false,
    } as ReturnType<typeof useEffectivePermission>);

    const { result } = renderHook(() => usePermission('app-1'), {
      wrapper: createWrapper(),
    });

    expect(result.current.can('view')).toBe(true);
    expect(result.current.can('operate')).toBe(true);
    expect(result.current.can('edit')).toBe(true);
    expect(result.current.can('manage')).toBe(false);
    expect(result.current.can('owner')).toBe(false);
  });

  it('should default to "none" when data is null-ish', () => {
    mockedUseEffectivePermission.mockReturnValue({
      data: null,
      isLoading: false,
    } as unknown as ReturnType<typeof useEffectivePermission>);

    const { result } = renderHook(() => usePermission('app-1'), {
      wrapper: createWrapper(),
    });

    expect(result.current.level).toBe('none');
    expect(result.current.can('view')).toBe(false);
  });
});
