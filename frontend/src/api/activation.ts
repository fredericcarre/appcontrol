import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface ActivationStatus {
  level: number;
  name: string;
  description: string;
  allows_checks: boolean;
  allows_ops: boolean;
  requires_pr_approval: boolean;
}

export interface ActivationResponse {
  application_id: string;
  activation: ActivationStatus;
  available_levels: ActivationStatus[];
}

export function useActivation(appId: string | undefined) {
  return useQuery({
    queryKey: ['activation', appId],
    queryFn: async () => {
      const res = await client.get<ActivationResponse>(`/apps/${appId}/activation`);
      return res.data;
    },
    enabled: !!appId,
    staleTime: 60_000,
  });
}

export function useSetActivation(appId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (level: number) => {
      const res = await client.put(`/apps/${appId}/activation`, { level });
      return res.data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['activation', appId] });
      qc.invalidateQueries({ queryKey: ['app', appId] });
      qc.invalidateQueries({ queryKey: ['apps'] });
    },
  });
}
