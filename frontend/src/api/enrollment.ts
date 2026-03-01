import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

// ── Types ──────────────────────────────────────────────────────

export interface EnrollmentToken {
  id: string;
  name: string;
  token: string;
  scope: 'agent' | 'gateway';
  max_uses: number | null;
  current_uses: number;
  expires_at: string | null;
  status: 'active' | 'revoked' | 'expired' | 'exhausted';
  created_by: string;
  created_at: string;
  revoked_at: string | null;
}

export interface CreateEnrollmentTokenPayload {
  name: string;
  scope: 'agent' | 'gateway';
  max_uses?: number | null;
  valid_hours?: number;
}

export interface CreateEnrollmentTokenResponse {
  id: string;
  name: string;
  token: string;
  scope: 'agent' | 'gateway';
  max_uses: number | null;
  expires_at: string | null;
  status: 'active';
  created_at: string;
}

export interface EnrollmentEvent {
  id: string;
  token_id: string;
  token_name: string;
  event_type: string;
  agent_id: string | null;
  gateway_id: string | null;
  hostname: string | null;
  ip_address: string | null;
  details: Record<string, unknown>;
  created_at: string;
}

// ── Queries ────────────────────────────────────────────────────

export function useEnrollmentTokens() {
  return useQuery({
    queryKey: ['enrollment', 'tokens'],
    queryFn: async () => {
      const { data } = await client.get<EnrollmentToken[]>('/enrollment/tokens');
      return data;
    },
  });
}

export function useEnrollmentEvents() {
  return useQuery({
    queryKey: ['enrollment', 'events'],
    queryFn: async () => {
      const { data } = await client.get<EnrollmentEvent[]>('/enrollment/events');
      return data;
    },
  });
}

// ── Mutations ──────────────────────────────────────────────────

export function useCreateEnrollmentToken() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: CreateEnrollmentTokenPayload) => {
      const { data } = await client.post<CreateEnrollmentTokenResponse>('/enrollment/tokens', payload);
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['enrollment', 'tokens'] }),
  });
}

export function useRevokeEnrollmentToken() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (tokenId: string) => {
      await client.post(`/enrollment/tokens/${tokenId}/revoke`);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['enrollment', 'tokens'] });
      qc.invalidateQueries({ queryKey: ['enrollment', 'events'] });
    },
  });
}
