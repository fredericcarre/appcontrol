import { Check, ShieldCheck, HelpCircle, Settings2 } from 'lucide-react';
import { cn } from '@/lib/utils';
import { useDiscoveryStore } from '@/stores/discovery';
import type { ServiceConfidence } from './TopologyMap.types';
import { getConfidenceInfo } from './confidence';

interface ConfidenceFilterButtonProps {
  level: ServiceConfidence;
  count: number;
  selected: boolean;
  onToggle: () => void;
}

function ConfidenceFilterButton({ level, count, selected, onToggle }: ConfidenceFilterButtonProps) {
  const info = getConfidenceInfo(level);

  const icons: Record<ServiceConfidence, React.ReactNode> = {
    recognized: <ShieldCheck className="h-3.5 w-3.5" />,
    likely: <Check className="h-3.5 w-3.5" />,
    unknown: <HelpCircle className="h-3.5 w-3.5" />,
    system: <Settings2 className="h-3.5 w-3.5" />,
  };

  return (
    <button
      onClick={onToggle}
      className={cn(
        'flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs font-medium transition-all',
        'border',
        selected
          ? `${info.bgColor} ${info.borderColor} ${info.color}`
          : 'bg-background border-border text-muted-foreground hover:bg-muted'
      )}
      title={info.description}
    >
      <div
        className={cn(
          'w-4 h-4 rounded flex items-center justify-center transition-colors',
          selected ? info.color : 'text-muted-foreground'
        )}
      >
        {icons[level]}
      </div>
      <span>{info.label}</span>
      <span
        className={cn(
          'ml-0.5 px-1.5 py-0.5 rounded text-[10px] font-mono',
          selected ? 'bg-white/50' : 'bg-muted'
        )}
      >
        {count}
      </span>
    </button>
  );
}

export function ConfidenceFilterBar() {
  const selectedConfidenceLevels = useDiscoveryStore((s) => s.selectedConfidenceLevels);
  const toggleConfidenceFilter = useDiscoveryStore((s) => s.toggleConfidenceFilter);
  const getConfidenceCounts = useDiscoveryStore((s) => s.getConfidenceCounts);

  const counts = getConfidenceCounts();
  const levels: ServiceConfidence[] = ['recognized', 'likely', 'unknown', 'system'];

  return (
    <div className="flex items-center gap-1.5">
      <span className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mr-1">
        Show:
      </span>
      {levels.map((level) => (
        <ConfidenceFilterButton
          key={level}
          level={level}
          count={counts[level]}
          selected={selectedConfidenceLevels.has(level)}
          onToggle={() => toggleConfidenceFilter(level)}
        />
      ))}
    </div>
  );
}
