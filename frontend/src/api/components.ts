import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface CommandExecution {
  id: string;
  request_id: string;
  component_id: string;
  command_type: string;
  status: string;
  exit_code: number | null;
  stdout: string | null;
  stderr: string | null;
  duration_ms: number | null;
  dispatched_at: string;
  completed_at: string | null;
}

export interface CommandDispatchResult {
  request_id: string;
  command: string;
  status: string;
  component_id: string;
  agent_id: string;
}

export interface CustomCommand {
  id: string;
  component_id: string;
  name: string;
  command: string;
  description: string | null;
  requires_confirmation: boolean;
  min_permission_level: string;
}

export interface CommandInputParam {
  id: string;
  command_id: string;
  name: string;
  description: string | null;
  default_value: string | null;
  validation_regex: string | null;
  required: boolean;
  display_order: number;
  param_type: string;
  enum_values: string[] | null;
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

export function useCustomCommands(componentId: string) {
  return useQuery({
    queryKey: ['components', componentId, 'commands'],
    queryFn: async () => {
      const { data } = await client.get<{ commands: CustomCommand[] }>(
        `/components/${componentId}/commands`,
      );
      return data.commands;
    },
    enabled: !!componentId,
  });
}

export function useCommandParams(commandId: string | null) {
  return useQuery({
    queryKey: ['commands', commandId, 'params'],
    queryFn: async () => {
      const { data } = await client.get<{ params: CommandInputParam[] }>(
        `/commands/${commandId}/params`,
      );
      return data.params;
    },
    enabled: !!commandId,
  });
}

export interface StateTransition {
  id: string;
  component_id: string;
  from_state: string;
  to_state: string;
  trigger: string;
  created_at: string;
}

export function useStateTransitions(componentId: string, limit = 30) {
  return useQuery({
    queryKey: ['components', componentId, 'state-transitions'],
    queryFn: async () => {
      const { data } = await client.get<{ transitions: StateTransition[] }>(
        `/components/${componentId}/state-transitions?limit=${limit}`,
      );
      return data.transitions;
    },
    enabled: !!componentId,
    refetchInterval: 15_000,
  });
}

export function useCommandExecutions(componentId: string, limit = 20) {
  return useQuery({
    queryKey: ['components', componentId, 'command-executions'],
    queryFn: async () => {
      const { data } = await client.get<{ executions: CommandExecution[] }>(
        `/components/${componentId}/command-executions?limit=${limit}`,
      );
      return data.executions;
    },
    enabled: !!componentId,
    refetchInterval: 10_000,
  });
}

export function useExecuteCommand() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (payload: {
      component_id: string;
      command_type: string;
      parameters?: Record<string, string>;
    }) => {
      const { data } = await client.post<CommandDispatchResult>(
        `/components/${payload.component_id}/command/${payload.command_type}`,
        { parameters: payload.parameters },
      );
      return data;
    },
    onSuccess: (_, vars) => {
      qc.invalidateQueries({ queryKey: ['components', vars.component_id] });
      qc.invalidateQueries({ queryKey: ['components', vars.component_id, 'command-executions'] });
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

export function useForceStopComponent() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (componentId: string) => {
      const { data } = await client.post(`/components/${componentId}/force-stop`);
      return data;
    },
    onSuccess: (_, id) => qc.invalidateQueries({ queryKey: ['components', id] }),
  });
}

export function useStartWithDeps() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (componentId: string) => {
      const { data } = await client.post(`/components/${componentId}/start-with-deps`);
      return data;
    },
    onSuccess: (_, id) => qc.invalidateQueries({ queryKey: ['components', id] }),
  });
}
