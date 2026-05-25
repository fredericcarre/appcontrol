import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

export type AnnotationTarget = 'application' | 'component' | 'dependency';
export type AnnotationKind = 'note' | 'review' | 'todo' | 'warning';

export interface Annotation {
  id: string;
  organization_id: string;
  target_type: AnnotationTarget;
  target_id: string;
  kind: AnnotationKind;
  body: string;
  metadata: Record<string, unknown>;
  author_id: string | null;
  resolved_at: string | null;
  resolved_by: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateAnnotationInput {
  target_type: AnnotationTarget;
  target_id: string;
  kind?: AnnotationKind;
  body: string;
  metadata?: Record<string, unknown>;
}

const key = (type: AnnotationTarget, id: string, includeResolved: boolean) =>
  ['annotations', type, id, includeResolved] as const;

export function useAnnotations(
  target_type: AnnotationTarget | undefined,
  target_id: string | undefined,
  includeResolved = false,
) {
  return useQuery({
    queryKey: key(target_type ?? 'component', target_id ?? '', includeResolved),
    queryFn: async () => {
      const res = await client.get<{ annotations: Annotation[]; total: number }>(
        '/annotations',
        { params: { target_type, target_id, include_resolved: includeResolved } },
      );
      return res.data;
    },
    enabled: !!target_type && !!target_id,
    staleTime: 15_000,
  });
}

export function useCreateAnnotation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (input: CreateAnnotationInput) => {
      const res = await client.post('/annotations', input);
      return res.data;
    },
    onSuccess: (_data, input) => {
      qc.invalidateQueries({ queryKey: ['annotations', input.target_type, input.target_id] });
    },
  });
}

export function useResolveAnnotation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const res = await client.post(`/annotations/${id}/resolve`);
      return res.data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['annotations'] });
    },
  });
}

export function useDeleteAnnotation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const res = await client.delete(`/annotations/${id}`);
      return res.data;
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['annotations'] });
    },
  });
}
