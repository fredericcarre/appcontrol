import { useKnowledgeSummary, KnowledgeStatus } from '@/api/knowledge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { cn } from '@/lib/utils';

interface Props {
  appId: string | undefined;
  className?: string;
}

/**
 * Headline knowledge maturity card for an application. Renders the
 * validated-coverage percentage prominently and a small breakdown by
 * status. Designed to sit on the app detail page next to the
 * activation level card.
 */
export function KnowledgeSummaryCard({ appId, className }: Props) {
  const { data, isLoading } = useKnowledgeSummary(appId);

  if (isLoading) {
    return (
      <Card className={cn('border-slate-200', className)}>
        <CardContent className="p-4 text-xs text-slate-500">
          Calcul de la maturité de connaissance…
        </CardContent>
      </Card>
    );
  }
  if (!data) {
    return null;
  }

  const coverage = Math.round(data.validated_coverage * 100);
  return (
    <Card className={cn('border-slate-200', className)}>
      <CardHeader className="pb-2">
        <CardTitle className="flex items-center justify-between text-sm">
          <span>Maturité de la connaissance</span>
          <span className="text-[10px] uppercase tracking-wider text-slate-500">
            {data.component_total} composants
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex items-baseline gap-2">
          <span className="text-3xl font-bold text-emerald-700">{coverage}%</span>
          <span className="text-xs text-slate-500">validés</span>
        </div>

        <div>
          <div className="mb-1 flex justify-between text-[10px] uppercase tracking-wider text-slate-500">
            <span>Composants</span>
            <span>{data.component_total} total</span>
          </div>
          <StatusBar counts={data.components_by_status} />
        </div>

        <div>
          <div className="mb-1 flex justify-between text-[10px] uppercase tracking-wider text-slate-500">
            <span>Dépendances</span>
          </div>
          <StatusBar counts={data.dependencies_by_status} />
        </div>
      </CardContent>
    </Card>
  );
}

function StatusBar({
  counts,
}: {
  counts: { knowledge_status: KnowledgeStatus; count: number }[];
}) {
  const total = counts.reduce((s, c) => s + c.count, 0);
  if (total === 0) {
    return (
      <div className="h-2 rounded-full bg-slate-100" />
    );
  }
  const order: KnowledgeStatus[] = ['validated', 'reviewed', 'draft', 'candidate', 'deprecated'];
  const palette: Record<KnowledgeStatus, string> = {
    validated: 'bg-emerald-500',
    reviewed: 'bg-indigo-500',
    draft: 'bg-amber-400',
    candidate: 'bg-slate-300',
    deprecated: 'bg-red-400',
  };
  return (
    <div className="flex h-2 w-full overflow-hidden rounded-full bg-slate-100">
      {order.map((status) => {
        const c = counts.find((x) => x.knowledge_status === status);
        if (!c || c.count === 0) return null;
        const pct = (c.count / total) * 100;
        return (
          <div
            key={status}
            className={palette[status]}
            style={{ width: `${pct}%` }}
            title={`${status}: ${c.count}`}
          />
        );
      })}
    </div>
  );
}
