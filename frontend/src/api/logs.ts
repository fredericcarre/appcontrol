import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

// ── Types ──────────────────────────────────────────────────────

export interface LogSource {
  id: string;
  component_id: string;
  name: string;
  source_type: 'file' | 'event_log' | 'process';
  // For file sources
  file_path?: string;
  // For event_log sources (Windows)
  log_name?: string;
  event_source?: string;
  // Common
  is_enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface LogEntry {
  timestamp: string | null;
  level: string | null;
  content: string;
}

export interface LogsResponse {
  source_type: string;
  source_name: string;
  entries: LogEntry[];
  total_lines: number;
  truncated: boolean;
}

export interface CreateLogSourceInput {
  name: string;
  source_type: 'file' | 'event_log';
  file_path?: string;
  log_name?: string;
  event_source?: string;
}

export interface UpdateLogSourceInput {
  name?: string;
  file_path?: string;
  log_name?: string;
  event_source?: string;
  is_enabled?: boolean;
}

// ── Component Log Sources ──────────────────────────────────────

export function useComponentLogSources(componentId: string) {
  return useQuery({
    queryKey: ['components', componentId, 'log-sources'],
    queryFn: async () => {
      const { data } = await client.get<LogSource[]>(
        `/components/${componentId}/log-sources`
      );
      return data;
    },
    enabled: !!componentId,
    refetchInterval: 60000, // Refresh every 60s
  });
}

export function useCreateLogSource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({
      componentId,
      ...payload
    }: CreateLogSourceInput & { componentId: string }) => {
      const { data } = await client.post<LogSource>(
        `/components/${componentId}/log-sources`,
        payload
      );
      return data;
    },
    onSuccess: (_, vars) => {
      qc.invalidateQueries({
        queryKey: ['components', vars.componentId, 'log-sources'],
      });
    },
  });
}

export function useUpdateLogSource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({
      id,
      componentId,
      ...payload
    }: UpdateLogSourceInput & { id: string; componentId: string }) => {
      const { data } = await client.put<LogSource>(
        `/log-sources/${id}`,
        payload
      );
      return { data, componentId };
    },
    onSuccess: (result) => {
      qc.invalidateQueries({
        queryKey: ['components', result.componentId, 'log-sources'],
      });
    },
  });
}

export function useDeleteLogSource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({
      id,
      componentId,
    }: {
      id: string;
      componentId: string;
    }) => {
      await client.delete(`/log-sources/${id}`);
      return { componentId };
    },
    onSuccess: (result) => {
      qc.invalidateQueries({
        queryKey: ['components', result.componentId, 'log-sources'],
      });
    },
  });
}

// ── Log Retrieval ──────────────────────────────────────────────

export interface GetLogsParams {
  source?: string; // 'process' or log source UUID
  lines?: number;
  filter?: string;
  since?: string; // '1h', '24h', '7d'
}

export function useComponentLogs(
  componentId: string,
  params: GetLogsParams = {}
) {
  const { source, lines = 100, filter, since } = params;

  return useQuery({
    queryKey: ['components', componentId, 'logs', params],
    queryFn: async () => {
      const searchParams = new URLSearchParams();
      if (source) searchParams.set('source', source);
      if (lines) searchParams.set('lines', lines.toString());
      if (filter) searchParams.set('filter', filter);
      if (since) searchParams.set('since', since);

      const { data } = await client.get<LogsResponse>(
        `/components/${componentId}/logs?${searchParams.toString()}`
      );
      return data;
    },
    enabled: !!componentId,
    refetchInterval: 10000, // Refresh every 10s for near-realtime
  });
}

// ── Utility Functions ──────────────────────────────────────────

export function getSourceTypeIcon(sourceType: LogSource['source_type']): string {
  switch (sourceType) {
    case 'file':
      return 'FileText';
    case 'event_log':
      return 'Monitor';
    case 'process':
      return 'Terminal';
    default:
      return 'File';
  }
}

export function getSourceTypeLabel(sourceType: LogSource['source_type']): string {
  switch (sourceType) {
    case 'file':
      return 'File';
    case 'event_log':
      return 'Event Log';
    case 'process':
      return 'Process Output';
    default:
      return sourceType;
  }
}

export function getLevelColor(level: string | null): string {
  switch (level?.toUpperCase()) {
    case 'ERROR':
      return 'text-red-600 dark:text-red-400';
    case 'WARN':
    case 'WARNING':
      return 'text-yellow-600 dark:text-yellow-400';
    case 'INFO':
      return 'text-blue-600 dark:text-blue-400';
    case 'DEBUG':
      return 'text-gray-500 dark:text-gray-400';
    default:
      return 'text-gray-700 dark:text-gray-300';
  }
}

export function getLevelBadgeColor(level: string | null): string {
  switch (level?.toUpperCase()) {
    case 'ERROR':
      return 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200';
    case 'WARN':
    case 'WARNING':
      return 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200';
    case 'INFO':
      return 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200';
    case 'DEBUG':
      return 'bg-gray-100 text-gray-800 dark:bg-gray-800 dark:text-gray-200';
    default:
      return 'bg-gray-100 text-gray-800 dark:bg-gray-800 dark:text-gray-200';
  }
}
