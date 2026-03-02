import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

// ── Types ──────────────────────────────────────────────────────

export interface EnrollmentToken {
  id: string;
  name: string;
  token_prefix: string;
  scope: 'agent' | 'gateway';
  max_uses: number | null;
  current_uses: number;
  expires_at: string | null;
  created_at: string;
  revoked_at: string | null;
}

// Computed status based on token fields
export function getTokenStatus(token: EnrollmentToken): 'active' | 'revoked' | 'expired' | 'exhausted' {
  if (token.revoked_at) return 'revoked';
  if (token.expires_at && new Date(token.expires_at) < new Date()) return 'expired';
  if (token.max_uses !== null && token.current_uses >= token.max_uses) return 'exhausted';
  return 'active';
}

export interface CreateEnrollmentTokenPayload {
  name: string;
  scope: 'agent' | 'gateway';
  max_uses?: number | null;
  valid_hours?: number;
  zone?: string | null;
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
      const { data } = await client.get<{ tokens: EnrollmentToken[] }>('/enrollment/tokens');
      return data.tokens;
    },
  });
}

export function useEnrollmentEvents() {
  return useQuery({
    queryKey: ['enrollment', 'events'],
    queryFn: async () => {
      const { data } = await client.get<{ events: EnrollmentEvent[] }>('/enrollment/events');
      return data.events;
    },
  });
}

// ── PKI API ──────────────────────────────────────────────────

export interface PkiStatus {
  initialized: boolean;
  ca_fingerprint?: string;
}

export function usePkiStatus() {
  return useQuery({
    queryKey: ['pki', 'status'],
    queryFn: async () => {
      try {
        const { data } = await client.get<{ ca_cert_pem: string; fingerprint: string }>('/pki/ca');
        return { initialized: true, ca_fingerprint: data.fingerprint };
      } catch {
        return { initialized: false };
      }
    },
  });
}

export function useInitPki() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { org_name: string; validity_days?: number }) => {
      const { data } = await client.post<{ status: string; ca_fingerprint: string; validity_days: number }>(
        '/pki/init',
        payload
      );
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['pki', 'status'] }),
  });
}

export function useImportPki() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { ca_cert_pem: string; ca_key_pem: string; force?: boolean }) => {
      const { data } = await client.post<{ status: string; ca_fingerprint: string }>(
        '/pki/import',
        payload
      );
      return data;
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['pki', 'status'] }),
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
