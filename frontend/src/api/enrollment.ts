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
  enrolled_agents?: number;
  enrolled_gateways?: number;
  pending_rotation?: boolean;
  pending_ca_fingerprint?: string;
  rotation_started_at?: string;
}

export interface RotationProgress {
  rotation_id: string;
  organization_id: string;
  status: 'in_progress' | 'ready' | 'completed' | 'cancelled' | 'failed';
  total_agents: number;
  total_gateways: number;
  migrated_agents: number;
  migrated_gateways: number;
  failed_agents: number;
  failed_gateways: number;
  started_at: string;
  completed_at: string | null;
  finalized_at: string | null;
  grace_period_secs: number;
  old_ca_fingerprint: string | null;
  new_ca_fingerprint: string | null;
}

export function usePkiStatus() {
  return useQuery({
    queryKey: ['pki', 'status'],
    queryFn: async () => {
      try {
        const { data } = await client.get<{
          ca_initialized: boolean;
          ca_fingerprint: string | null;
          enrolled_agents: number;
          enrolled_gateways: number;
          pending_rotation: boolean;
          pending_ca_fingerprint: string | null;
          rotation_started_at: string | null;
        }>('/pki/status');
        return {
          initialized: data.ca_initialized,
          ca_fingerprint: data.ca_fingerprint ?? undefined,
          enrolled_agents: data.enrolled_agents,
          enrolled_gateways: data.enrolled_gateways,
          pending_rotation: data.pending_rotation,
          pending_ca_fingerprint: data.pending_ca_fingerprint ?? undefined,
          rotation_started_at: data.rotation_started_at ?? undefined,
        };
      } catch {
        // Fallback to old endpoint for backwards compatibility
        try {
          const { data } = await client.get<{ ca_cert_pem: string; fingerprint: string }>('/pki/ca');
          return { initialized: true, ca_fingerprint: data.fingerprint };
        } catch {
          return { initialized: false };
        }
      }
    },
  });
}

export function useRotationProgress() {
  return useQuery({
    queryKey: ['pki', 'rotation', 'progress'],
    queryFn: async () => {
      const { data } = await client.get<{ progress: RotationProgress | null }>('/pki/rotation/progress');
      return data.progress;
    },
    refetchInterval: 5000, // Poll every 5 seconds during rotation
  });
}

export function useStartRotation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { new_ca_cert_pem: string; new_ca_key_pem: string; grace_period_secs?: number }) => {
      const { data } = await client.post<{ status: string; rotation_id: string; progress: RotationProgress }>(
        '/pki/rotation/start',
        payload
      );
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['pki', 'status'] });
      qc.invalidateQueries({ queryKey: ['pki', 'rotation', 'progress'] });
    },
  });
}

export function useFinalizeRotation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const { data } = await client.post<{ status: string }>('/pki/rotation/finalize');
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['pki', 'status'] });
      qc.invalidateQueries({ queryKey: ['pki', 'rotation', 'progress'] });
    },
  });
}

export function useCancelRotation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const { data } = await client.post<{ status: string }>('/pki/rotation/cancel');
      return data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['pki', 'status'] });
      qc.invalidateQueries({ queryKey: ['pki', 'rotation', 'progress'] });
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
