import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export interface Pattern {
  id: string;
  organization_id: string;
  name: string;
  technology: string;
  description: string | null;
  check_cmd_template: string | null;
  integrity_check_cmd_template: string | null;
  infra_check_cmd_template: string | null;
  start_cmd_template: string | null;
  stop_cmd_template: string | null;
  rebuild_cmd_template: string | null;
  tags: string[];
  created_from_incident_id: string | null;
  is_enabled: boolean;
  usage_count: number;
  created_by: string | null;
  created_at: string;
  updated_at: string;
}

export interface PatternCandidate {
  component_id: string;
  component_name: string;
  application_id: string;
  application_name: string;
  component_type: string;
  has_check_cmd: boolean;
}

export function usePatterns(technology?: string) {
  return useQuery({
    queryKey: ['patterns', technology ?? null],
    queryFn: async () => {
      const res = await client.get<{ patterns: Pattern[]; total: number }>(
        '/patterns',
        { params: { technology } },
      );
      return res.data;
    },
  });
}

export function usePatternCandidates(patternId: string | undefined) {
  return useQuery({
    queryKey: ['patterns', patternId, 'candidates'],
    queryFn: async () => {
      const res = await client.get<{
        pattern_id: string;
        technology: string;
        candidates: PatternCandidate[];
        total: number;
      }>(`/patterns/${patternId}/candidates`);
      return res.data;
    },
    enabled: !!patternId,
  });
}

export function usePropagatePattern() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, componentIds }: { id: string; componentIds: string[] }) => {
      const res = await client.post(`/patterns/${id}/propagate`, {
        component_ids: componentIds,
      });
      return res.data;
    },
    onSuccess: (_d, vars) => {
      qc.invalidateQueries({ queryKey: ['patterns'] });
      qc.invalidateQueries({ queryKey: ['patterns', vars.id, 'candidates'] });
    },
  });
}
