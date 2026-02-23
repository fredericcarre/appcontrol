import { useQuery } from '@tanstack/react-query';
import client from './client';

export interface UserSearchResult {
  id: string;
  email: string;
  display_name: string | null;
  role: string;
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
