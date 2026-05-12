import { useMemo, useState } from 'react';
import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  Loader2,
  Play,
  Square,
} from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  useDryRunStart,
  useDryRunStop,
  type DryRunResponse,
} from '@/api/apps';

interface DryRunDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  appId: string;
  appName?: string;
  /** Forwarded into the actual Start All / Stop All buttons in the toolbar
   *  so operators can confirm-then-execute without re-clicking through the
   *  full dialog chain. The dialog only triggers these once the operator
   *  has reviewed the plan. */
  onConfirmStart?: () => void;
  onConfirmStop?: () => void;
  /** Whether the operator currently has permission to execute the
   *  real start/stop (mirrors `canOperate` on the parent map). */
  canOperate?: boolean;
}

/**
 * "What would happen if I clicked Start All / Stop All?"
 *
 * Shows the operator the exact plan the backend sequencer would walk —
 * DAG topological levels, components per level — *without* dispatching
 * anything. Same code path as the real start/stop (the `dry_run: true`
 * flag short-circuits before any agent message is sent), so the plan is
 * always in sync with what the live operation would do.
 *
 * Two tabs: Start (Level 1 → Level N) and Stop (reverse). The Start view
 * is the default since "Will my Start All work?" is the common question.
 *
 * The dialog itself doesn't trigger execution — operators close it and
 * use the existing Start All / Stop All toolbar buttons. We could add a
 * "Run it now" shortcut here, but that risks an accidental click during
 * review; keeping confirmation in the existing toolbar flow is safer.
 */
export function DryRunDialog({
  open,
  onOpenChange,
  appId,
  appName,
  onConfirmStart,
  onConfirmStop,
  canOperate,
}: DryRunDialogProps) {
  const [direction, setDirection] = useState<'start' | 'stop'>('start');
  const dryStart = useDryRunStart();
  const dryStop = useDryRunStop();

  // Fetch the plan the first time the dialog opens for a given direction.
  // We rely on react-query's mutation cache so flipping back and forth
  // doesn't re-hit the backend in the same dialog session.
  const fetchPlan = (next: 'start' | 'stop') => {
    setDirection(next);
    if (next === 'start') {
      if (!dryStart.data && !dryStart.isPending) dryStart.mutate(appId);
    } else {
      if (!dryStop.data && !dryStop.isPending) dryStop.mutate(appId);
    }
  };

  // Kick off the start fetch when the dialog first opens.
  useMemo(() => {
    if (open && !dryStart.data && !dryStart.isPending) {
      dryStart.mutate(appId);
    }
    // We only want to fire on `open` transitioning to true, not on every
    // mutate identity change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, appId]);

  const currentMutation = direction === 'start' ? dryStart : dryStop;
  const currentPlan: DryRunResponse | undefined = currentMutation.data;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            Dry run — what would happen if you{' '}
            {direction === 'start' ? 'Started' : 'Stopped'} this app?
          </DialogTitle>
          <DialogDescription>
            The DAG is laid out in topological order. Within a level,
            components are dispatched in parallel; the sequencer waits for
            every component in level <em>N</em> to be healthy before it
            kicks off level <em>N+1</em>. No commands are sent right now —
            this is a preview.
          </DialogDescription>
        </DialogHeader>

        {/* Direction switch */}
        <div className="inline-flex rounded-md border p-0.5 self-start text-xs">
          <button
            type="button"
            onClick={() => fetchPlan('start')}
            className={
              'px-3 py-1 rounded ' +
              (direction === 'start'
                ? 'bg-emerald-600 text-white'
                : 'text-muted-foreground hover:bg-muted')
            }
          >
            <Play className="inline h-3 w-3 mr-1" />
            Start plan
          </button>
          <button
            type="button"
            onClick={() => fetchPlan('stop')}
            className={
              'px-3 py-1 rounded ' +
              (direction === 'stop'
                ? 'bg-red-600 text-white'
                : 'text-muted-foreground hover:bg-muted')
            }
          >
            <Square className="inline h-3 w-3 mr-1" />
            Stop plan
          </button>
        </div>

        <ScrollArea className="max-h-[60vh] mt-2 rounded border">
          {currentMutation.isPending ? (
            <div className="flex items-center justify-center h-32 text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin mr-2" />
              Computing plan…
            </div>
          ) : currentMutation.isError ? (
            <div className="flex flex-col items-center justify-center h-32 gap-1 text-red-700">
              <AlertCircle className="h-5 w-5" />
              <span className="text-sm">
                Couldn't compute the {direction} plan.
              </span>
              <span className="text-xs text-muted-foreground">
                {(currentMutation.error as Error)?.message ?? 'Unknown error'}
              </span>
            </div>
          ) : currentPlan ? (
            <PlanView plan={currentPlan} direction={direction} />
          ) : null}
        </ScrollArea>

        <DialogFooter className="gap-2 sm:gap-2">
          {direction === 'start' && onConfirmStart && canOperate && (
            <Button
              variant="default"
              className="bg-emerald-600 hover:bg-emerald-700"
              onClick={() => {
                onOpenChange(false);
                onConfirmStart();
              }}
            >
              <Play className="h-4 w-4 mr-1" />
              Run this Start
            </Button>
          )}
          {direction === 'stop' && onConfirmStop && canOperate && (
            <Button
              variant="destructive"
              onClick={() => {
                onOpenChange(false);
                onConfirmStop();
              }}
            >
              <Square className="h-4 w-4 mr-1" />
              Run this Stop
            </Button>
          )}
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Close
          </Button>
        </DialogFooter>
        {appName && (
          <p className="text-[10px] text-muted-foreground text-center -mt-2">
            Application: <span className="font-mono">{appName}</span>
          </p>
        )}
      </DialogContent>
    </Dialog>
  );
}

function PlanView({
  plan,
  direction,
}: {
  plan: DryRunResponse;
  direction: 'start' | 'stop';
}) {
  // Stop walks the levels in reverse — render that explicit so the
  // operator doesn't have to mentally invert the order. Backend already
  // returns the start-order levels; we flip locally for the Stop view.
  const ordered =
    direction === 'stop' ? [...plan.plan.levels].reverse() : plan.plan.levels;

  if (ordered.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-32 text-muted-foreground">
        <AlertCircle className="h-5 w-5 mb-2" />
        <span className="text-sm">No startable components found.</span>
      </div>
    );
  }

  return (
    <ol className="p-3 space-y-3 text-sm">
      {ordered.map((level, idx) => {
        // Display number = idx+1 for start (Level 1 → N), but for stop we
        // want the original level number so the operator can match it
        // back to the start plan.
        const displayLevelNumber =
          direction === 'stop' ? plan.plan.total_levels - idx : idx + 1;
        return (
          <li
            key={`${direction}-${idx}`}
            className="rounded border bg-background p-2"
          >
            <div className="flex items-center gap-2 mb-1 text-xs font-semibold">
              <Badge variant="outline" className="text-[10px]">
                {direction === 'start' ? (
                  <ArrowDown className="h-3 w-3 mr-0.5" />
                ) : (
                  <ArrowUp className="h-3 w-3 mr-0.5" />
                )}
                Step {idx + 1}
              </Badge>
              <span className="text-muted-foreground">
                Level {displayLevelNumber} · {level.length} component
                {level.length !== 1 ? 's' : ''} in parallel
              </span>
            </div>
            <ul className="flex flex-wrap gap-1.5 pl-1">
              {level.map((c) => (
                <li
                  key={c.component_id}
                  className="text-xs font-mono px-1.5 py-0.5 rounded bg-muted"
                  title={c.component_id}
                >
                  {c.name}
                </li>
              ))}
            </ul>
          </li>
        );
      })}
    </ol>
  );
}

export default DryRunDialog;
