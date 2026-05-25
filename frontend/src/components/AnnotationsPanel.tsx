import { useState } from 'react';
import {
  AnnotationKind,
  AnnotationTarget,
  useAnnotations,
  useCreateAnnotation,
  useDeleteAnnotation,
  useResolveAnnotation,
} from '@/api/annotations';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { cn } from '@/lib/utils';

interface Props {
  targetType: AnnotationTarget;
  targetId: string;
  className?: string;
}

/**
 * Self-contained panel that lists annotations on a target, lets the
 * user create new ones (note / review / todo / warning) and resolve
 * them. Backed by the /api/v1/annotations endpoints.
 */
export function AnnotationsPanel({ targetType, targetId, className }: Props) {
  const [includeResolved, setIncludeResolved] = useState(false);
  const [draftBody, setDraftBody] = useState('');
  const [draftKind, setDraftKind] = useState<AnnotationKind>('note');

  const { data, isLoading } = useAnnotations(targetType, targetId, includeResolved);
  const createMutation = useCreateAnnotation();
  const resolveMutation = useResolveAnnotation();
  const deleteMutation = useDeleteAnnotation();

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!draftBody.trim()) return;
    createMutation.mutate(
      { target_type: targetType, target_id: targetId, kind: draftKind, body: draftBody.trim() },
      {
        onSuccess: () => {
          setDraftBody('');
          setDraftKind('note');
        },
      },
    );
  };

  return (
    <Card className={cn('border-slate-200', className)}>
      <CardHeader className="pb-2">
        <CardTitle className="flex items-center justify-between text-sm">
          <span>Annotations</span>
          <button
            type="button"
            className="text-xs font-normal text-slate-500 hover:text-slate-800"
            onClick={() => setIncludeResolved((v) => !v)}
          >
            {includeResolved ? 'Masquer résolues' : 'Afficher résolues'}
          </button>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <form onSubmit={onSubmit} className="space-y-2">
          <div className="flex gap-1">
            {(['note', 'review', 'todo', 'warning'] as AnnotationKind[]).map((k) => (
              <button
                key={k}
                type="button"
                onClick={() => setDraftKind(k)}
                className={cn(
                  'rounded-md border px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider',
                  draftKind === k
                    ? kindPalettes[k]
                    : 'border-slate-200 bg-white text-slate-500 hover:bg-slate-50',
                )}
              >
                {k}
              </button>
            ))}
          </div>
          <textarea
            value={draftBody}
            onChange={(e) => setDraftBody(e.target.value)}
            placeholder="Ajoute une note, une review, un todo ou un avertissement…"
            rows={2}
            className="w-full resize-none rounded-md border border-slate-200 bg-white p-2 text-sm focus:border-teal-500 focus:outline-none"
          />
          <div className="flex justify-end">
            <Button
              type="submit"
              size="sm"
              disabled={!draftBody.trim() || createMutation.isPending}
            >
              Publier
            </Button>
          </div>
        </form>

        {isLoading && <p className="text-xs text-slate-500">Chargement…</p>}
        {data?.annotations.length === 0 && (
          <p className="text-xs text-slate-500">Aucune annotation pour le moment.</p>
        )}

        <ul className="space-y-2">
          {data?.annotations.map((a) => (
            <li
              key={a.id}
              className={cn(
                'rounded-md border bg-white p-3 text-sm',
                a.resolved_at ? 'border-slate-200 opacity-60' : 'border-slate-300',
              )}
            >
              <div className="flex items-start justify-between gap-2">
                <div className="flex-1">
                  <div className="mb-1 flex items-center gap-2">
                    <Badge
                      variant="outline"
                      className={cn('text-[10px] font-semibold uppercase', kindPalettes[a.kind])}
                    >
                      {a.kind}
                    </Badge>
                    <span className="text-[10px] text-slate-500">
                      {new Date(a.created_at).toLocaleString()}
                    </span>
                    {a.resolved_at && (
                      <Badge variant="outline" className="text-[10px] text-slate-500">
                        résolu
                      </Badge>
                    )}
                  </div>
                  <p className="whitespace-pre-wrap text-slate-700">{a.body}</p>
                </div>
                <div className="flex flex-col gap-1">
                  {!a.resolved_at && (
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => resolveMutation.mutate(a.id)}
                      disabled={resolveMutation.isPending}
                    >
                      Résoudre
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => deleteMutation.mutate(a.id)}
                    disabled={deleteMutation.isPending}
                  >
                    Supprimer
                  </Button>
                </div>
              </div>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}

const kindPalettes: Record<AnnotationKind, string> = {
  note: 'border-slate-300 bg-slate-100 text-slate-700',
  review: 'border-indigo-300 bg-indigo-50 text-indigo-800',
  todo: 'border-amber-300 bg-amber-50 text-amber-800',
  warning: 'border-red-300 bg-red-50 text-red-700',
};
