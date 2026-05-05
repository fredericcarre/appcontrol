import { useState } from 'react';
import { Check, SkipForward, AlertTriangle, Clock } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Badge } from '@/components/ui/badge';
import {
  useManualTask,
  useValidateManualTask,
  type ManualTaskValidation,
} from '@/api/manualTasks';

interface ManualTaskPanelProps {
  componentId: string;
  canOperate?: boolean;
}

/**
 * Side-panel tab for components whose `component_type === 'manual_task'`.
 *
 * Layout:
 *   * The component's `manual_description` (markdown shown raw — the renderer
 *     can be upgraded to a markdown library later, but raw monospace already
 *     handles the URL/screenshot-link case the operator pastes in).
 *   * A pending validation block (if any) with a comment textarea and three
 *     buttons: Validate (green), Skip (amber), Failed (red). Whichever the
 *     operator clicks closes the pending row and unblocks the sequencer.
 *   * A history list of past validations with who/when/duration/status —
 *     this is the audit trail the user asked for.
 *
 * The hook polls every 2 s while a row is pending and every 30 s otherwise,
 * so the panel reflects "the sequencer reached me, you can validate now"
 * without the operator hitting refresh.
 */
export function ManualTaskPanel({ componentId, canOperate }: ManualTaskPanelProps) {
  const { data, isLoading } = useManualTask(componentId);
  const validate = useValidateManualTask(componentId);
  const [comment, setComment] = useState('');

  if (isLoading) {
    return <div className="p-4 text-sm text-muted-foreground">Loading…</div>;
  }

  const pending = data?.history.find((h) => h.status === 'pending');

  const submit = (status: 'validated' | 'skipped' | 'failed') => {
    validate.mutate(
      { status, comment: comment.trim() ? comment.trim() : undefined },
      {
        onSuccess: () => setComment(''),
      },
    );
  };

  return (
    <div className="flex flex-col gap-3 p-4">
      {/* Description */}
      <section>
        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1">
          Manual task description
        </h4>
        {data?.manual_description ? (
          <div className="text-sm whitespace-pre-wrap rounded border p-3 bg-muted/30">
            {data.manual_description}
          </div>
        ) : (
          <div className="text-sm italic text-muted-foreground">
            No description set. Edit the component to add the instructions
            (markdown supported — paste image URLs or hyperlinks for
            screenshots and runbook references).
          </div>
        )}
      </section>

      {/* Pending validation */}
      {pending ? (
        <section className="rounded border-2 border-amber-300 bg-amber-50 dark:bg-amber-950/30 p-3">
          <div className="flex items-center gap-2 text-sm font-medium text-amber-700 dark:text-amber-300">
            <Clock className="h-4 w-4" />
            Pending validation since{' '}
            {new Date(pending.started_at).toLocaleString()}
          </div>
          <p className="text-xs text-muted-foreground mt-1">
            The DAG is paused on this component. Validate to advance, Skip
            to advance without claiming the task succeeded, or Failed to
            stop the sequence here.
          </p>

          <textarea
            className="w-full text-sm rounded border bg-background p-2 mt-2 font-mono"
            placeholder="Comment (e.g. ticket #, who you contacted, what was confirmed) — required for the audit log"
            rows={3}
            value={comment}
            onChange={(e) => setComment(e.target.value)}
            disabled={!canOperate || validate.isPending}
          />

          <div className="flex flex-wrap gap-2 mt-2">
            <Button
              size="sm"
              className="bg-emerald-600 hover:bg-emerald-700"
              disabled={!canOperate || validate.isPending}
              onClick={() => submit('validated')}
            >
              <Check className="mr-1 h-3.5 w-3.5" />
              Validate
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={!canOperate || validate.isPending}
              onClick={() => submit('skipped')}
            >
              <SkipForward className="mr-1 h-3.5 w-3.5" />
              Skip
            </Button>
            <Button
              size="sm"
              variant="destructive"
              disabled={!canOperate || validate.isPending}
              onClick={() => {
                if (
                  confirm(
                    'Marking this manual task as failed will fail the DAG step. Continue?',
                  )
                ) {
                  submit('failed');
                }
              }}
            >
              <AlertTriangle className="mr-1 h-3.5 w-3.5" />
              Failed
            </Button>
          </div>
          {!canOperate && (
            <p className="text-[11px] text-muted-foreground mt-2">
              Operate permission required to validate.
            </p>
          )}
        </section>
      ) : (
        <section className="rounded border bg-muted/20 p-3 text-sm text-muted-foreground">
          No validation currently pending. The next time this component is
          started by the sequencer, a pending entry will appear here.
        </section>
      )}

      {/* History */}
      <section>
        <h4 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-1">
          Audit history (most recent first)
        </h4>
        <ScrollArea className="max-h-[300px] rounded-md border">
          {!data?.history.length ? (
            <div className="p-4 text-sm text-muted-foreground text-center">
              No validations recorded yet.
            </div>
          ) : (
            <ul className="divide-y">
              {data.history.map((h) => (
                <HistoryRow key={h.id} v={h} />
              ))}
            </ul>
          )}
        </ScrollArea>
      </section>
    </div>
  );
}

function HistoryRow({ v }: { v: ManualTaskValidation }) {
  const colour =
    v.status === 'validated'
      ? 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400'
      : v.status === 'skipped'
        ? 'bg-amber-500/15 text-amber-700 dark:text-amber-400'
        : v.status === 'failed'
          ? 'bg-red-500/15 text-red-700 dark:text-red-400'
          : 'bg-blue-500/15 text-blue-700 dark:text-blue-400';

  return (
    <li className="p-3 text-sm">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <Badge className={colour}>{v.status.toUpperCase()}</Badge>
        <span className="text-xs text-muted-foreground">
          started {new Date(v.started_at).toLocaleString()}
        </span>
      </div>
      <div className="text-xs text-muted-foreground mt-1">
        {v.validated_at ? (
          <>
            closed {new Date(v.validated_at).toLocaleString()}
            {v.duration_seconds != null && ` · ${formatDuration(v.duration_seconds)}`}
            {v.validated_by && ` · by ${v.validated_by.slice(0, 8)}`}
          </>
        ) : (
          <span className="italic">still pending</span>
        )}
      </div>
      {v.comment && (
        <div className="mt-2 rounded bg-muted/40 p-2 text-xs whitespace-pre-wrap font-mono">
          {v.comment}
        </div>
      )}
    </li>
  );
}

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  if (minutes < 60) return `${minutes}m ${rest}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}
