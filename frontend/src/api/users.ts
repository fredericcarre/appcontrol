import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface UserSearchResult {
  id: string;
  email: string;
  display_name: string | null;
  role: string;
}

export interface User {
  id: string;
  organization_id: string;
  email: string;
  display_name: string | null;
  role: string;
  auth_provider: string;
  is_active: boolean;
  last_login_at: string | null;
  created_at: string;
}

export function useSearchUsers(query: string, enabled = true) {
  return useQuery({
    queryKey: ['users', 'search', query],
    queryFn: async () => {
      const { data } = await client.get<{ users: UserSearchResult[] }>('/users/search', {
        params: { q: query, limit: 20 },
      });
      return data.users;
    },
    enabled: enabled && query.length >= 1,
    staleTime: 30_000,
  });
}

export function useUsers() {
  return useQuery({
    queryKey: ['users'],
    queryFn: async () => {
      const { data } = await client.get<{ users: User[] }>('/users');
      return data.users;
    },
  });
}

export function useCreateUser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: {
      email: string;
      display_name: string;
      role: string;
      password?: string;
    }) => {
      const { data } = await client.post<User>('/users', payload);
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['users'] }),
  });
}

export function useUpdateUser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: {
      id: string;
      display_name?: string;
      role?: string;
      password?: string;
    }) => {
      const { id, ...body } = payload;
      const { data } = await client.put<User>(`/users/${id}`, body);
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['users'] }),
  });
}

export function useToggleUserActive() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ userId, is_active }: { userId: string; is_active: boolean }) => {
      await client.put(`/users/${userId}`, { is_active });
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['users'] }),
  });
}

export function useChangePassword() {
  return useMutation({
    mutationFn: async ({ currentPassword, newPassword }: { currentPassword: string; newPassword: string }) => {
      const { data } = await client.post<{ status: string; message: string }>('/users/me/password', {
        current_password: currentPassword,
        new_password: newPassword,
      });
      return data;
    },
  });
}
