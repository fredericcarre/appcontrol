import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export type KnowledgeStatus =
  | 'candidate'
  | 'draft'
  | 'reviewed'
  | 'validated'
  | 'deprecated';

export interface KnowledgeUpdate {
  confidence_score?: number;
  knowledge_status?: KnowledgeStatus;
}

export interface KnowledgeStatusCount {
  knowledge_status: KnowledgeStatus;
  count: number;
}

export interface KnowledgeSummary {
  application_id: string;
  components_by_status: KnowledgeStatusCount[];
  dependencies_by_status: KnowledgeStatusCount[];
  component_total: number;
  component_validated: number;
  validated_coverage: number;
}

export function useUpdateComponentKnowledge() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, body }: { id: string; body: KnowledgeUpdate }) => {
      const res = await client.put(`/components/${id}/knowledge`, body);
      return res.data;
    },
    onSuccess: (_d, vars) => {
      qc.invalidateQueries({ queryKey: ['component', vars.id] });
      qc.invalidateQueries({ queryKey: ['knowledge'] });
    },
  });
}

export function useUpdateDependencyKnowledge() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, body }: { id: string; body: KnowledgeUpdate }) => {
      const res = await client.put(`/dependencies/${id}/knowledge`, body);
      return res.data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['knowledge'] });
    },
  });
}

export function useKnowledgeSummary(appId: string | undefined) {
  return useQuery({
    queryKey: ['knowledge', 'summary', appId],
    queryFn: async () => {
      const res = await client.get<KnowledgeSummary>(`/apps/${appId}/knowledge/summary`);
      return res.data;
    },
    enabled: !!appId,
    staleTime: 60_000,
  });
}
