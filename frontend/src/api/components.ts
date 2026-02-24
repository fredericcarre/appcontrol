import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface CommandExecution {
  id: string;
  component_id: string;
  command_type: string;
  status: string;
  output: string;
  exit_code: number | null;
  started_at: string;
  completed_at: string | null;
}

export function useComponentState(componentId: string) {
  return useQuery({
    queryKey: ['components', componentId, 'state'],
    queryFn: async () => {
      const { data } = await client.get<{ state: string }>(`/components/${componentId}/state`);
      return data;
    },
    enabled: !!componentId,
    refetchInterval: 10_000,
  });
}

export function useExecuteCommand() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: { component_id: string; command_type: string; args?: string[] }) => {
      const { data } = await client.post<CommandExecution>(
        `/components/${payload.component_id}/command/${payload.command_type}`,
        { args: payload.args },
      );
      return data;
    },
    onSuccess: (_, vars) => {
      qc.invalidateQueries({ queryKey: ['components', vars.component_id] });
    },
  });
}

export function useStartComponent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (componentId: string) => {
      const { data } = await client.post(`/components/${componentId}/start`);
      return data;
    },
    onSuccess: (_, id) => qc.invalidateQueries({ queryKey: ['components', id] }),
  });
}

export function useStopComponent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (componentId: string) => {
      const { data } = await client.post(`/components/${componentId}/stop`);
      return data;
    },
    onSuccess: (_, id) => qc.invalidateQueries({ queryKey: ['components', id] }),
  });
}

export function useDiagnoseComponent() {
  return useMutation({
    mutationFn: async (payload: { component_id: string; levels: number[] }) => {
      const { data } = await client.post(`/components/${payload.component_id}/diagnose`, payload);
      return data;
    },
  });
}
