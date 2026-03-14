import { useMemo } from 'react';
import { Plus, GripVertical, HelpCircle, Settings2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';
import { useDiscoveryStore } from '@/stores/discovery';
import { classifyConfidence, getConfidenceInfo } from './confidence';
import type { ServiceConfidence } from './TopologyMap.types';

interface StagingServiceProps {
  index: number;
  onInclude: () => void;
}

function StagingService({ index, onInclude }: StagingServiceProps) {
  const correlationResult = useDiscoveryStore((s) => s.correlationResult);
  const service = correlationResult?.services[index];

  if (!service) return null;

  const confidence = classifyConfidence(service);
  const confidenceInfo = getConfidenceInfo(confidence);
  const techHint = service.technology_hint;
  const displayName = techHint?.display_name || service.process_name;

  return (
    <div
      className={cn(
        'flex items-center gap-2 px-2 py-1.5 rounded-md border bg-card',
        'hover:shadow-sm transition-all cursor-grab active:cursor-grabbing',
        confidenceInfo.borderColor
      )}
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData('serviceIndex', String(index));
        e.dataTransfer.effectAllowed = 'move';
      }}
    >
      <GripVertical className="h-3 w-3 text-muted-foreground flex-shrink-0" />

      {/* Confidence indicator */}
      <div
        className={cn('w-2 h-2 rounded-full flex-shrink-0', confidenceInfo.bgColor)}
        style={{
          backgroundColor:
            confidence === 'recognized'
              ? '#10b981'
              : confidence === 'likely'
                ? '#f59e0b'
                : confidence === 'system'
                  ? '#94a3b8'
                  : '#cbd5e1',
        }}
      />

      {/* Service name */}
      <span className="text-xs font-medium truncate flex-1" title={displayName}>
        {displayName}
      </span>

      {/* Port badge */}
      {service.ports.length > 0 && (
        <Badge variant="secondary" className="text-[9px] px-1 py-0 h-4 font-mono">
          :{service.ports[0]}
        </Badge>
      )}

      {/* Include button */}
      <Button
        size="icon"
        variant="ghost"
        className="h-5 w-5 text-emerald-600 hover:bg-emerald-50"
        onClick={(e) => {
          e.stopPropagation();
          onInclude();
        }}
        title="Include in map"
      >
        <Plus className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}

export function StagingArea() {
  const correlationResult = useDiscoveryStore((s) => s.correlationResult);
  const enabledServiceIndices = useDiscoveryStore((s) => s.enabledServiceIndices);
  const selectedConfidenceLevels = useDiscoveryStore((s) => s.selectedConfidenceLevels);
  const toggleServiceEnabled = useDiscoveryStore((s) => s.toggleServiceEnabled);

  // Get services that are NOT enabled (in staging)
  const stagedServices = useMemo(() => {
    if (!correlationResult) return [];

    const staged: Array<{ index: number; confidence: ServiceConfidence }> = [];
    correlationResult.services.forEach((svc, i) => {
      // Only show services that are not currently enabled
      if (!enabledServiceIndices.has(i)) {
        const confidence = classifyConfidence(svc);
        // Only show if the confidence level is selected in filters
        if (selectedConfidenceLevels.has(confidence)) {
          staged.push({ index: i, confidence });
        }
      }
    });

    // Sort by confidence: unknown first (needs attention), then system
    return staged.sort((a, b) => {
      const order: Record<ServiceConfidence, number> = {
        unknown: 0,
        likely: 1,
        recognized: 2,
        system: 3,
      };
      return order[a.confidence] - order[b.confidence];
    });
  }, [correlationResult, enabledServiceIndices, selectedConfidenceLevels]);

  const handleInclude = (index: number) => {
    toggleServiceEnabled(index);
  };

  // Handle drop from staging to map (this will be handled by the drop zone in TopologyMap)
  // For now, this is the source of draggable items

  if (stagedServices.length === 0) {
    return null;
  }

  // Group by confidence
  const unknownCount = stagedServices.filter((s) => s.confidence === 'unknown').length;
  const systemCount = stagedServices.filter((s) => s.confidence === 'system').length;
  const otherCount = stagedServices.length - unknownCount - systemCount;

  return (
    <div className="absolute bottom-3 left-1/2 -translate-x-1/2 z-10">
      <div className="bg-card/95 backdrop-blur-sm border border-border rounded-lg shadow-lg px-3 py-2 max-w-2xl">
        {/* Header */}
        <div className="flex items-center gap-2 mb-2">
          <span className="text-[10px] font-semibold text-muted-foreground uppercase tracking-wider">
            Staging Area
          </span>
          <div className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
            {unknownCount > 0 && (
              <span className="flex items-center gap-0.5">
                <HelpCircle className="h-3 w-3 text-slate-400" />
                {unknownCount}
              </span>
            )}
            {systemCount > 0 && (
              <span className="flex items-center gap-0.5">
                <Settings2 className="h-3 w-3 text-slate-400" />
                {systemCount}
              </span>
            )}
            {otherCount > 0 && <span>{otherCount} other</span>}
          </div>
          <span className="text-[10px] text-muted-foreground ml-auto">
            Drag to include or click +
          </span>
        </div>

        {/* Service list */}
        <ScrollArea className="max-h-24">
          <div className="flex flex-wrap gap-1.5">
            {stagedServices.slice(0, 20).map(({ index }) => (
              <StagingService
                key={index}
                index={index}
                onInclude={() => handleInclude(index)}
              />
            ))}
            {stagedServices.length > 20 && (
              <div className="flex items-center px-2 text-[10px] text-muted-foreground">
                +{stagedServices.length - 20} more
              </div>
            )}
          </div>
        </ScrollArea>
      </div>
    </div>
  );
}
