import { Component } from '@/api/apps';
import {
  KnowledgeStatus,
  useUpdateComponentKnowledge,
} from '@/api/knowledge';
import { KnowledgeBadge } from '@/components/KnowledgeBadge';
import { AnnotationsPanel } from '@/components/AnnotationsPanel';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { cn } from '@/lib/utils';

interface Props {
  component: Component;
  canEdit: boolean;
}

/**
 * Knowledge tab on the component detail panel — central artefact of
 * the methodology's Phase 3 (human review) and the running journal
 * filled during Phase 5 (learning from incidents).
 *
 * Layout:
 *   1. Current status + confidence with promotion buttons.
 *   2. Free-form annotations panel (notes, reviews, todos, warnings).
 *
 * The status flow is irreversible-by-default in UX (you don't usually
 * go backward) but reversible via the API. We offer linear promotion
 * here; reverts go through the API directly or admin tools.
 */
export function KnowledgeTab({ component, canEdit }: Props) {
  const update = useUpdateComponentKnowledge();
  const status = (component.knowledge_status ?? 'draft') as KnowledgeStatus;
  const confidence = component.confidence_score ?? 0.5;

  const promotionFlow: KnowledgeStatus[] = [
    'candidate',
    'draft',
    'reviewed',
    'validated',
  ];

  return (
    <div className="flex flex-col gap-4 p-4 overflow-auto">
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="flex items-center justify-between text-sm">
            <span>Statut de connaissance</span>
            <KnowledgeBadge status={status} confidence={confidence} />
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-xs text-slate-600">
            Avance ce composant dans la trajectoire de revue. Chaque
            promotion est tracée dans l'audit log.
          </p>

          <ol className="flex items-center gap-1">
            {promotionFlow.map((s, idx) => (
              <li key={s} className="flex flex-1 items-center gap-1">
                <button
                  type="button"
                  disabled={!canEdit || update.isPending || s === status}
                  onClick={() =>
                    update.mutate({
                      id: component.id,
                      body: { knowledge_status: s },
                    })
                  }
                  className={cn(
                    'flex-1 rounded-md border px-2 py-1.5 text-[10px] font-semibold uppercase tracking-wider transition',
                    s === status
                      ? statusActive[s]
                      : 'border-slate-200 bg-white text-slate-500 hover:border-slate-400 hover:text-slate-700',
                    !canEdit && 'opacity-50 cursor-not-allowed',
                  )}
                >
                  {idx + 1}. {s}
                </button>
                {idx < promotionFlow.length - 1 && (
                  <span className="text-slate-300">→</span>
                )}
              </li>
            ))}
          </ol>

          <div>
            <label className="flex items-center justify-between text-xs text-slate-600">
              <span>Score de confiance</span>
              <span className="font-mono text-slate-900">
                {Math.round(confidence * 100)} %
              </span>
            </label>
            <input
              type="range"
              min={0}
              max={100}
              step={5}
              value={Math.round(confidence * 100)}
              disabled={!canEdit || update.isPending}
              onChange={(e) =>
                update.mutate({
                  id: component.id,
                  body: {
                    confidence_score: Number(e.target.value) / 100,
                  },
                })
              }
              className="mt-1 w-full accent-teal-600"
            />
            <p className="mt-1 text-[10px] text-slate-500">
              0 % = candidat brut, 100 % = parfaitement aligné avec la
              réalité observée et validé par l'équipe.
            </p>
          </div>

          {status !== 'validated' && status !== 'deprecated' && (
            <Button
              size="sm"
              variant="outline"
              disabled={!canEdit || update.isPending}
              onClick={() =>
                update.mutate({
                  id: component.id,
                  body: { knowledge_status: 'deprecated' },
                })
              }
              className="w-full"
            >
              Marquer comme déprécié
            </Button>
          )}
        </CardContent>
      </Card>

      <AnnotationsPanel
        targetType="component"
        targetId={component.id}
      />
    </div>
  );
}

const statusActive: Record<KnowledgeStatus, string> = {
  candidate: 'border-slate-400 bg-slate-100 text-slate-800',
  draft: 'border-amber-400 bg-amber-50 text-amber-800',
  reviewed: 'border-indigo-400 bg-indigo-50 text-indigo-800',
  validated: 'border-emerald-400 bg-emerald-50 text-emerald-800',
  deprecated: 'border-red-300 bg-red-50 text-red-700',
};
