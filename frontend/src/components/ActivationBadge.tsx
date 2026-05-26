import { ActivationStatus } from '@/api/activation';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

interface Props {
  status: ActivationStatus | undefined;
  className?: string;
}

/**
 * Compact badge that surfaces the current activation level of an application.
 * Mirrors the colour scheme used in the strategy / vision documents:
 *   level 0 captation  → slate
 *   level 1 advisory   → amber (warn)
 *   level 2 diagnostic → indigo
 *   level 3 PR-only    → teal
 *   level 4 direct ops → green
 */
export function ActivationBadge({ status, className }: Props) {
  if (!status) {
    return (
      <Badge variant="secondary" className={cn('text-[10px]', className)} title="Activation inconnue">
        — / 4
      </Badge>
    );
  }

  const palette = paletteForLevel(status.level);

  return (
    <Badge
      variant="outline"
      className={cn(
        'text-[10px] font-semibold uppercase tracking-wider',
        palette,
        className,
      )}
      title={status.description}
    >
      Lvl {status.level} · {status.name}
    </Badge>
  );
}

function paletteForLevel(level: number): string {
  switch (level) {
    case 0:
      return 'border-slate-300 bg-slate-100 text-slate-700';
    case 1:
      return 'border-amber-300 bg-amber-50 text-amber-800';
    case 2:
      return 'border-indigo-300 bg-indigo-50 text-indigo-800';
    case 3:
      return 'border-teal-300 bg-teal-50 text-teal-800';
    case 4:
      return 'border-emerald-300 bg-emerald-50 text-emerald-800';
    default:
      return 'border-slate-300 bg-slate-50 text-slate-700';
  }
}
