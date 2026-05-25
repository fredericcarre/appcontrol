import { Badge } from '@/components/ui/badge';
import { KnowledgeStatus } from '@/api/knowledge';
import { cn } from '@/lib/utils';

interface Props {
  status: KnowledgeStatus | undefined;
  confidence?: number | null;
  className?: string;
}

/**
 * Compact badge showing how validated a component / dependency is.
 * Palette mirrors the methodology document:
 *   candidate   → slate    (raw captation output)
 *   draft       → amber    (under human review)
 *   reviewed    → indigo   (peer-reviewed)
 *   validated   → emerald  (signed off)
 *   deprecated  → red-ish  (about to be removed)
 */
export function KnowledgeBadge({ status, confidence, className }: Props) {
  if (!status) {
    return (
      <Badge variant="secondary" className={cn('text-[10px]', className)}>
        ?
      </Badge>
    );
  }
  const palette = palettes[status];
  const confidenceLabel =
    confidence !== undefined && confidence !== null
      ? ` · ${Math.round(confidence * 100)}%`
      : '';
  return (
    <Badge
      variant="outline"
      className={cn(
        'text-[10px] font-semibold uppercase tracking-wider',
        palette,
        className,
      )}
      title={`Knowledge: ${status}${confidenceLabel}`}
    >
      {status}
      {confidenceLabel}
    </Badge>
  );
}

const palettes: Record<KnowledgeStatus, string> = {
  candidate: 'border-slate-300 bg-slate-100 text-slate-700',
  draft: 'border-amber-300 bg-amber-50 text-amber-800',
  reviewed: 'border-indigo-300 bg-indigo-50 text-indigo-800',
  validated: 'border-emerald-300 bg-emerald-50 text-emerald-800',
  deprecated: 'border-red-300 bg-red-50 text-red-700 line-through',
};
