import { Link } from 'react-router-dom';
import { Clock, ChevronRight } from 'lucide-react';
import { usePendingManualTasks } from '@/api/manualTasks';

/**
 * Dashboard banner that surfaces all manual-task validations currently
 * waiting on the operator (across every app the user has Operate on).
 * Renders nothing when the queue is empty so it doesn't clutter the
 * "everything is fine" view.
 *
 * Each row is a deep-link to the component's map view with the side panel
 * pre-opened on its Manual task tab — saves the operator from drilling
 * down through the dashboard → app → component path.
 *
 * The hook polls every 15 s so a sequencer that just paused on a manual
 * task surfaces here within a quarter-minute, even before the operator
 * navigates back to the dashboard.
 */
export function PendingManualTasksBanner() {
  const { data, isLoading } = usePendingManualTasks();

  if (isLoading || !data || data.count === 0) return null;

  return (
    <div
      className="rounded-lg border-2 border-amber-300 bg-amber-50 dark:bg-amber-950/20 p-4"
      role="region"
      aria-label="Pending manual tasks"
    >
      <div className="flex items-start gap-3">
        <Clock className="h-5 w-5 text-amber-600 mt-0.5 shrink-0" />
        <div className="flex-1 min-w-0">
          <h2 className="text-sm font-semibold text-amber-800 dark:text-amber-200">
            {data.count} manual task{data.count !== 1 ? 's' : ''} awaiting validation
          </h2>
          <p className="text-xs text-amber-700/80 dark:text-amber-200/80 mt-0.5">
            The DAG is paused on each of these. Open the component to validate, skip, or fail.
          </p>
          <ul className="mt-3 divide-y divide-amber-200/70 dark:divide-amber-900/50 rounded border border-amber-200 dark:border-amber-900 bg-white/60 dark:bg-black/30">
            {data.tasks.slice(0, 6).map((t) => {
              const componentLabel = t.component_display_name || t.component_name;
              const waited = formatWait(t.started_at);
              return (
                <li key={t.validation_id}>
                  <Link
                    to={`/apps/${t.application_id}?selected=${t.component_id}`}
                    className="flex items-center justify-between gap-2 px-3 py-2 text-sm hover:bg-amber-100/70 dark:hover:bg-amber-950/40"
                  >
                    <div className="min-w-0">
                      <span className="font-medium">{componentLabel}</span>
                      <span className="text-muted-foreground"> · {t.application_name}</span>
                    </div>
                    <div className="flex items-center gap-2 text-xs text-muted-foreground shrink-0">
                      <span>waiting {waited}</span>
                      <ChevronRight className="h-3.5 w-3.5" />
                    </div>
                  </Link>
                </li>
              );
            })}
          </ul>
          {data.count > 6 && (
            <p className="text-xs text-muted-foreground mt-2">
              + {data.count - 6} more — open each component from its app map.
            </p>
          )}
        </div>
      </div>
    </div>
  );
}

function formatWait(startedAt: string): string {
  const seconds = Math.floor((Date.now() - new Date(startedAt).getTime()) / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}
