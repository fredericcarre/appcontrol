import { GitBranch, MousePointer2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { useDiscoveryStore } from '@/stores/discovery';

export function DependencyModeToggle() {
  const { dependencyMode, setDependencyMode, pendingDependency, setPendingDependency } = useDiscoveryStore();

  const isCreateMode = dependencyMode === 'create';

  const handleToggle = () => {
    if (isCreateMode) {
      // Exit create mode, clear any pending
      setDependencyMode('view');
      setPendingDependency(null);
    } else {
      setDependencyMode('create');
    }
  };

  return (
    <Button
      variant={isCreateMode ? 'default' : 'outline'}
      size="sm"
      className={cn(
        'h-7 text-xs gap-1.5 transition-colors',
        isCreateMode && 'bg-emerald-600 hover:bg-emerald-700'
      )}
      onClick={handleToggle}
      title={isCreateMode ? 'Exit dependency creation mode (ESC)' : 'Create manual dependencies'}
    >
      {isCreateMode ? (
        <>
          <MousePointer2 className="h-3.5 w-3.5" />
          {pendingDependency ? 'Select Target...' : 'Creating Links'}
        </>
      ) : (
        <>
          <GitBranch className="h-3.5 w-3.5" />
          Link
        </>
      )}
    </Button>
  );
}
