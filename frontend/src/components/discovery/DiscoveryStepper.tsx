import { Check, Radar, Filter, Map, Rocket } from 'lucide-react';
import { cn } from '@/lib/utils';
import type { DiscoveryPhase } from './TopologyMap.types';

interface DiscoveryStepperProps {
  currentPhase: DiscoveryPhase;
  triageProgress?: number;
}

const STEPS = [
  { phase: 'scan' as const, label: 'Scan', icon: Radar, description: 'Scan agents' },
  { phase: 'triage' as const, label: 'Triage', icon: Filter, description: 'Sort components' },
  { phase: 'topology' as const, label: 'Build', icon: Map, description: 'Build the map' },
  { phase: 'done' as const, label: 'Done', icon: Rocket, description: 'App created' },
];

export function DiscoveryStepper({ currentPhase, triageProgress }: DiscoveryStepperProps) {
  const currentIndex = STEPS.findIndex((s) => s.phase === currentPhase);

  return (
    <div className="w-full max-w-3xl mx-auto">
      <div className="flex items-center justify-between">
        {STEPS.map((step, index) => {
          const isCompleted = index < currentIndex;
          const isCurrent = index === currentIndex;
          const Icon = step.icon;

          return (
            <div key={step.phase} className="flex items-center flex-1">
              {/* Step circle and label */}
              <div className="flex flex-col items-center">
                <div
                  className={cn(
                    'w-10 h-10 rounded-full flex items-center justify-center border-2 transition-all duration-300',
                    isCompleted && 'bg-emerald-500 border-emerald-500 text-white',
                    isCurrent && 'bg-primary border-primary text-primary-foreground',
                    !isCompleted && !isCurrent && 'bg-muted border-muted-foreground/30 text-muted-foreground'
                  )}
                >
                  {isCompleted ? (
                    <Check className="h-5 w-5" />
                  ) : (
                    <Icon className="h-5 w-5" />
                  )}
                </div>
                <div className="mt-2 text-center">
                  <div
                    className={cn(
                      'text-sm font-medium',
                      isCurrent && 'text-foreground',
                      isCompleted && 'text-emerald-600',
                      !isCompleted && !isCurrent && 'text-muted-foreground'
                    )}
                  >
                    {step.label}
                    {step.phase === 'triage' && isCurrent && triageProgress !== undefined && (
                      <span className="ml-1 text-xs font-normal text-primary">
                        {triageProgress}%
                      </span>
                    )}
                  </div>
                  <div className="text-[10px] text-muted-foreground hidden sm:block">
                    {step.description}
                  </div>
                </div>
              </div>

              {/* Connector line */}
              {index < STEPS.length - 1 && (
                <div className="flex-1 mx-4 relative">
                  <div className="h-0.5 bg-muted-foreground/20 w-full" />
                  <div
                    className={cn(
                      'absolute top-0 left-0 h-0.5 bg-emerald-500 transition-all duration-500',
                      isCompleted ? 'w-full' : 'w-0'
                    )}
                  />
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
