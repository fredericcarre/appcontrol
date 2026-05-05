import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import client from './client';

/**
 * Per-app map display options. Each flag controls whether a piece of metadata
 * is rendered on the component node. Absent / undefined means "show" (sensible
 * default — existing apps with empty `{}` keep their current visual fidelity).
 *
 * Add new flags here; the backend just stores the JSON as-is so no migration
 * is needed when extending this list.
 */
export interface MapDisplayOptions {
  show_host?: boolean;            // agent / host hostname under the title
  show_metrics?: boolean;         // metrics widget + indigo "N metrics" pill
  show_site_bindings?: boolean;   // multi-site split-panel
  show_cluster_badge?: boolean;   // x{N} aggregate / fan-out · X/Y
  show_weather?: boolean;         // app-level weather icon (toolbar)
  show_links?: boolean;           // hyperlinks shown under the action row
  density?: 'comfortable' | 'compact'; // future-proofing
}

const queryKey = (appId: string) => ['map-settings', appId] as const;

export function useMapSettings(appId: string | undefined) {
  return useQuery({
    queryKey: appId ? queryKey(appId) : ['map-settings', '_none'],
    queryFn: async () => {
      const { data } = await client.get<MapDisplayOptions>(`/apps/${appId}/map-settings`);
      return data ?? {};
    },
    enabled: !!appId,
  });
}

export function useUpdateMapSettings(appId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (next: MapDisplayOptions) => {
      const { data } = await client.put<MapDisplayOptions>(
        `/apps/${appId}/map-settings`,
        next,
      );
      return data;
    },
    // Optimistic — the menu flips instantly, refetch only on error.
    onMutate: async (next) => {
      await qc.cancelQueries({ queryKey: queryKey(appId) });
      const prev = qc.getQueryData<MapDisplayOptions>(queryKey(appId));
      qc.setQueryData(queryKey(appId), next);
      return { prev };
    },
    onError: (_err, _vars, ctx) => {
      if (ctx?.prev) qc.setQueryData(queryKey(appId), ctx.prev);
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: queryKey(appId) });
    },
  });
}

/**
 * Helper: read a flag with the implicit "absent = show" rule. Centralised so
 * every renderer applies the same convention.
 */
export function isFlagOn(opts: MapDisplayOptions | undefined, key: keyof MapDisplayOptions): boolean {
  if (!opts) return true;
  const v = opts[key];
  if (v === undefined) return true;
  return Boolean(v);
}
