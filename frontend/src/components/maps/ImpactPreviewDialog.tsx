import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { AlertTriangle, Play, Square, GitBranch, ArrowDown, ArrowUp, RotateCcw } from 'lucide-react';
import { cn } from '@/lib/utils';

interface ImpactedComponent {
  id: string;
  name: string;
  state?: string;
}

interface ImpactPreviewDialogProps {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  action: 'start' | 'stop' | 'start_with_deps' | 'restart_branch' | 'restart_with_dependents';
  componentName: string;
  impactedComponents: ImpactedComponent[];
}

export function ImpactPreviewDialog({
  open,
  onClose,
  onConfirm,
  action,
  componentName,
  impactedComponents,
}: ImpactPreviewDialogProps) {
  const hasImpact = impactedComponents.length > 0;

  const getActionConfig = () => {
    switch (action) {
      case 'start':
        return {
          title: 'Start Component',
          description: `Start "${componentName}"?`,
          icon: Play,
          iconColor: 'text-green-600',
          buttonText: 'Start',
          buttonVariant: 'default' as const,
          impactTitle: null,
          impactDescription: null,
          orderIcon: null,
        };
      case 'stop':
        return {
          title: 'Stop Component',
          description: `Stop "${componentName}"?`,
          icon: Square,
          iconColor: 'text-red-600',
          buttonText: 'Stop All',
          buttonVariant: 'destructive' as const,
          impactTitle: 'Components that will also stop',
          impactDescription: 'These components depend on the target and will be stopped first (in reverse dependency order):',
          orderIcon: ArrowUp,
        };
      case 'start_with_deps':
        return {
          title: 'Start with Dependencies',
          description: `Start "${componentName}" with all its dependencies?`,
          icon: GitBranch,
          iconColor: 'text-blue-600',
          buttonText: 'Start All',
          buttonVariant: 'default' as const,
          impactTitle: 'Dependencies that will start first',
          impactDescription: 'These components are required and will be started first (in dependency order):',
          orderIcon: ArrowDown,
        };
      case 'restart_branch':
        return {
          title: 'Repair Branch',
          description: `"${componentName}" is running but some dependencies are stopped. The component will be stopped first, then the entire branch will be restarted.`,
          icon: RotateCcw,
          iconColor: 'text-orange-600',
          buttonText: 'Repair Branch',
          buttonVariant: 'default' as const,
          impactTitle: 'Stopped dependencies to restart',
          impactDescription: 'These dependencies are stopped and need to be started. The component will be stopped first, then all dependencies will start in order:',
          orderIcon: ArrowDown,
        };
      case 'restart_with_dependents':
        return {
          title: 'Repair Component',
          description: `Restart "${componentName}" and all components that depend on it?`,
          icon: RotateCcw,
          iconColor: 'text-orange-600',
          buttonText: 'Repair',
          buttonVariant: 'default' as const,
          impactTitle: 'Dependent components that will be affected',
          impactDescription: 'These components depend on the target. They will be stopped first, then the target will restart, then they will restart:',
          orderIcon: ArrowUp,
        };
    }
  };

  const config = getActionConfig();
  const Icon = config.icon;
  const OrderIcon = config.orderIcon;

  return (
    <Dialog open={open} onOpenChange={(isOpen) => !isOpen && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Icon className={cn('h-5 w-5', config.iconColor)} />
            {config.title}
          </DialogTitle>
          <DialogDescription>{config.description}</DialogDescription>
        </DialogHeader>

        {hasImpact && config.impactTitle && (
          <div className="py-4">
            <div className="flex items-center gap-2 mb-2">
              <AlertTriangle className="h-4 w-4 text-amber-500" />
              <span className="font-medium text-sm">{config.impactTitle}</span>
            </div>
            <p className="text-xs text-muted-foreground mb-3">{config.impactDescription}</p>

            <div className="border rounded-md divide-y max-h-48 overflow-y-auto">
              {impactedComponents.map((comp, idx) => (
                <div key={comp.id} className="flex items-center gap-2 px-3 py-2 text-sm">
                  {OrderIcon && (
                    <span className="text-xs text-muted-foreground w-4">{idx + 1}</span>
                  )}
                  <span className="flex-1">{comp.name}</span>
                  {comp.state && (
                    <span className={cn(
                      'text-xs px-1.5 py-0.5 rounded',
                      comp.state === 'RUNNING' && 'bg-green-100 text-green-700',
                      comp.state === 'STOPPED' && 'bg-gray-100 text-gray-700',
                      comp.state === 'FAILED' && 'bg-red-100 text-red-700',
                      comp.state === 'STARTING' && 'bg-blue-100 text-blue-700',
                      comp.state === 'STOPPING' && 'bg-blue-100 text-blue-700',
                    )}>
                      {comp.state}
                    </span>
                  )}
                </div>
              ))}
            </div>

            <p className="text-xs text-muted-foreground mt-2 flex items-center gap-1">
              {action === 'stop' && (
                <>
                  <ArrowUp className="h-3 w-3" />
                  Stopping order: dependents first, then target
                </>
              )}
              {action === 'start_with_deps' && (
                <>
                  <ArrowDown className="h-3 w-3" />
                  Starting order: dependencies first, then target
                </>
              )}
              {action === 'restart_branch' && (
                <>
                  <RotateCcw className="h-3 w-3" />
                  Order: stop target → start dependencies → start target
                </>
              )}
              {action === 'restart_with_dependents' && (
                <>
                  <RotateCcw className="h-3 w-3" />
                  Order: stop dependents → stop target → start target → start dependents
                </>
              )}
            </p>
          </div>
        )}

        {!hasImpact && action === 'start' && (
          <div className="py-4">
            <p className="text-sm text-muted-foreground">
              This will start only "{componentName}". No other components will be affected.
            </p>
          </div>
        )}

        <DialogFooter className="gap-2 sm:gap-0">
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button variant={config.buttonVariant} onClick={onConfirm}>
            {config.buttonText}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
