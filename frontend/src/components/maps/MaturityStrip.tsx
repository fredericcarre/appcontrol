import { useActivation } from '@/api/activation';
import { useAnnotations } from '@/api/annotations';
import { useKnowledgeSummary } from '@/api/knowledge';
import { ActivationBadge } from '@/components/ActivationBadge';
import client from '@/api/client';
import { useQuery } from '@tanstack/react-query';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import { GitBranch, MessageSquare, GraduationCap } from 'lucide-react';

interface Props {
  appId: string;
  className?: string;
}

/**
 * Maturity strip — pins the four "where are we?" indicators of the
 * methodology at the top of an app's MapView:
 *
 *   1. Activation level     (phase 4 §5.1)
 *   2. Knowledge coverage   (phase 3 §4.4 + phase 4 §5.5)
 *   3. Open annotations     (phase 3 §4.4 review trail)
 *   4. Git sync status      (phase 3 §4.5 GitOps)
 *
 * The strip is read-only — clicking through opens the relevant
 * detail (activation page, knowledge tab, annotations, git settings).
 */
export function MaturityStrip({ appId, className }: Props) {
  const { data: activation } = useActivation(appId);
  const { data: knowledge } = useKnowledgeSummary(appId);
  const { data: annotations } = useAnnotations('application', appId, false);
  const { data: git } = useAppGit(appId);

  const coverage = knowledge?.validated_coverage ?? 0;
  const coveragePct = Math.round(coverage * 100);
  const openCount = annotations?.total ?? 0;

  return (
    <div className={cn('flex items-center gap-2 text-xs', className)}>
      <ActivationBadge status={activation?.activation} />

      <Badge
        variant="outline"
        className={cn(
          'flex items-center gap-1 text-[10px] font-semibold uppercase tracking-wider',
          coveragePct >= 80
            ? 'border-emerald-300 bg-emerald-50 text-emerald-800'
            : coveragePct >= 40
            ? 'border-indigo-300 bg-indigo-50 text-indigo-800'
            : 'border-amber-300 bg-amber-50 text-amber-800',
        )}
        title={`Connaissance validée : ${knowledge?.component_validated ?? 0}/${
          knowledge?.component_total ?? 0
        } composants`}
      >
        <GraduationCap className="h-3 w-3" />
        Knowledge {coveragePct}%
      </Badge>

      {openCount > 0 && (
        <Badge
          variant="outline"
          className="flex items-center gap-1 border-slate-300 bg-slate-50 text-[10px] font-semibold uppercase tracking-wider text-slate-700"
          title={`${openCount} annotation(s) ouverte(s) sur l'application et ses composants`}
        >
          <MessageSquare className="h-3 w-3" />
          {openCount} note{openCount > 1 ? 's' : ''}
        </Badge>
      )}

      {git?.git_remote_id && (
        <Badge
          variant="outline"
          className="flex items-center gap-1 border-teal-300 bg-teal-50 text-[10px] font-semibold uppercase tracking-wider text-teal-800"
          title={
            git.last_push_at
              ? `Git sync — dernier push : ${new Date(git.last_push_at).toLocaleString()}`
              : 'Git sync configuré — aucun push pour le moment'
          }
        >
          <GitBranch className="h-3 w-3" />
          Git
          {git.last_push_sha && (
            <span className="font-mono lowercase">
              {git.last_push_sha.slice(0, 7)}
            </span>
          )}
        </Badge>
      )}
    </div>
  );
}

interface AppGitResp {
  application_id: string;
  git_remote_id: string | null;
  path_override: string | null;
  auto_push_on_change: boolean;
  last_push_at: string | null;
  last_push_sha: string | null;
}

function useAppGit(appId: string) {
  return useQuery({
    queryKey: ['app-git', appId],
    queryFn: async () => {
      const res = await client.get<AppGitResp>(`/apps/${appId}/git`);
      return res.data;
    },
    enabled: !!appId,
    staleTime: 30_000,
  });
}
