import { useState } from 'react';
import { useSites } from '@/api/sites';
import {
  useSwitchoverStatus,
  useStartSwitchover,
  useAdvanceSwitchover,
  useRollbackSwitchover,
  useCommitSwitchover,
} from '@/api/switchover';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { RefreshCw, ArrowRightLeft, Check, X, AlertTriangle, Loader2, CheckCircle2 } from 'lucide-react';

interface SwitchoverPanelProps {
  appId: string;
  currentSiteId?: string | null;
  open: boolean;
  onClose: () => void;
  components?: Array<{ id: string; name: string; current_state: string }>;
  selectedComponentIds?: string[];
}

const PHASE_INFO: Record<string, { label: string; description: string }> = {
  PREPARE: {
    label: 'Preparing',
    description: 'Initializing switchover process and recording parameters',
  },
  VALIDATE: {
    label: 'Validating target',
    description: 'Checking binding profile exists and target agents are reachable',
  },
  STOP_SOURCE: {
    label: 'Stopping source',
    description: 'Stopping components in reverse DAG order (dependents first, then dependencies)',
  },
  SYNC: {
    label: 'Synchronizing',
    description: 'Data consistency checkpoint (future: install scripts on target)',
  },
  START_TARGET: {
    label: 'Starting target',
    description: 'Starting components in DAG order (dependencies first, then dependents)',
  },
  COMMIT: {
    label: 'Ready to commit',
    description: 'All components running on target site - confirm to finalize',
  },
  ROLLBACK: {
    label: 'Rolled back',
    description: 'Switchover was cancelled and reverted',
  },
};

const PHASE_ORDER = ['PREPARE', 'VALIDATE', 'STOP_SOURCE', 'SYNC', 'START_TARGET', 'COMMIT'];

export function SwitchoverPanel({ appId, currentSiteId, open, onClose, components, selectedComponentIds }: SwitchoverPanelProps) {
  const { data: sites } = useSites();
  const { data: status, refetch: refetchStatus } = useSwitchoverStatus(appId);
  const startSwitchover = useStartSwitchover();
  const advanceSwitchover = useAdvanceSwitchover();
  const rollbackSwitchover = useRollbackSwitchover();
  const commitSwitchover = useCommitSwitchover();

  const [targetSiteId, setTargetSiteId] = useState<string>('');
  const [mode, setMode] = useState<'FULL' | 'SELECTIVE'>('FULL');
  const [selectedIds, setSelectedIds] = useState<string[]>(selectedComponentIds || []);
  const [error, setError] = useState<string | null>(null);
  const [showRollbackConfirm, setShowRollbackConfirm] = useState(false);
  const [showCommitConfirm, setShowCommitConfirm] = useState(false);
  const [showSuccessDialog, setShowSuccessDialog] = useState(false);

  // Update selected IDs when prop changes
  useState(() => {
    if (selectedComponentIds?.length) {
      setSelectedIds(selectedComponentIds);
      setMode('SELECTIVE');
    }
  });

  const isActive = status?.current_status === 'in_progress';
  const currentPhase = status?.current_phase || '';
  const currentPhaseIndex = PHASE_ORDER.indexOf(currentPhase);

  // In SELECTIVE mode, show all active sites (component may be on a different site than the app)
  // In FULL mode, exclude the app's current site
  const availableSites = sites?.filter(s => {
    if (!s.is_active) return false;
    if (mode === 'SELECTIVE') return true; // Show all sites for selective switchover
    return s.id !== currentSiteId; // Exclude app's current site for full switchover
  }) || [];
  const currentSite = sites?.find(s => s.id === currentSiteId);

  const handleStart = async () => {
    if (!targetSiteId) return;
    if (mode === 'SELECTIVE' && selectedIds.length === 0) {
      setError('Select at least one component for selective switchover');
      return;
    }
    setError(null);
    try {
      await startSwitchover.mutateAsync({
        appId,
        target_site_id: targetSiteId,
        mode,
        component_ids: mode === 'SELECTIVE' ? selectedIds : undefined,
      });
      refetchStatus();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to start switchover');
    }
  };

  const handleAdvance = async () => {
    setError(null);
    try {
      await advanceSwitchover.mutateAsync(appId);
      refetchStatus();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to advance phase');
    }
  };

  const handleRollback = async () => {
    setShowRollbackConfirm(false);
    setError(null);
    try {
      await rollbackSwitchover.mutateAsync(appId);
      refetchStatus();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to rollback');
    }
  };

  const handleCommit = async () => {
    setShowCommitConfirm(false);
    setError(null);
    try {
      await commitSwitchover.mutateAsync(appId);
      refetchStatus();
      setShowSuccessDialog(true);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to commit');
    }
  };

  const isPending = startSwitchover.isPending || advanceSwitchover.isPending ||
    rollbackSwitchover.isPending || commitSwitchover.isPending;

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ArrowRightLeft className="h-5 w-5" />
            Site Switchover
          </DialogTitle>
          <DialogDescription>
            Failover application to a different site (DR, staging, etc.)
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          {/* Current site info */}
          <div className="rounded-lg border p-3 bg-muted/30">
            <div className="text-sm text-muted-foreground mb-1">Current Site</div>
            <div className="flex items-center gap-2">
              <div className={`w-3 h-3 rounded-full ${
                currentSite?.site_type === 'dr' ? 'bg-orange-500' : 'bg-green-500'
              }`} />
              <span className="font-medium">
                {currentSite?.name || 'Unknown'} ({currentSite?.code || '?'})
              </span>
            </div>
          </div>

          {/* Active switchover progress */}
          {isActive && (
            <div className="space-y-3">
              <div className="text-sm font-medium">Switchover in Progress</div>

              {/* Phase progress */}
              <div className="space-y-2">
                {PHASE_ORDER.map((phase, idx) => {
                  const isCompleted = idx < currentPhaseIndex;
                  const isCurrent = phase === currentPhase;
                  const isUpcoming = idx > currentPhaseIndex;
                  const phaseInfo = PHASE_INFO[phase] || { label: phase, description: '' };

                  return (
                    <div
                      key={phase}
                      className={`flex items-center gap-3 p-2 rounded transition-all ${
                        isCurrent ? 'bg-blue-50 border border-blue-200 dark:bg-blue-950/30 dark:border-blue-800' :
                        isCompleted ? 'bg-green-50 dark:bg-green-950/30' : 'bg-muted/30'
                      }`}
                    >
                      <div className={`w-7 h-7 rounded-full flex items-center justify-center flex-shrink-0 ${
                        isCompleted ? 'bg-green-500 text-white' :
                        isCurrent ? 'bg-blue-500 text-white' : 'bg-gray-300 dark:bg-gray-600'
                      }`}>
                        {isCompleted ? <Check className="h-4 w-4" /> :
                         isCurrent ? <Loader2 className="h-4 w-4 animate-spin" /> :
                         <span className="text-xs font-medium">{idx + 1}</span>}
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className={`text-sm font-medium ${isUpcoming ? 'text-muted-foreground' : ''}`}>
                          {phaseInfo.label}
                        </div>
                        {(isCurrent || isCompleted) && (
                          <div className="text-xs text-muted-foreground truncate">
                            {phaseInfo.description}
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>

              {/* Action buttons for active switchover */}
              <div className="flex gap-2 pt-2">
                {currentPhase === 'COMMIT' ? (
                  <Button onClick={() => setShowCommitConfirm(true)} disabled={isPending} className="flex-1">
                    {isPending ? <Loader2 className="h-4 w-4 animate-spin mr-2" /> : null}
                    Commit Switchover
                  </Button>
                ) : (
                  <Button onClick={handleAdvance} disabled={isPending} className="flex-1">
                    {isPending ? <Loader2 className="h-4 w-4 animate-spin mr-2" /> : null}
                    Next Phase
                  </Button>
                )}
                <Button variant="destructive" onClick={() => setShowRollbackConfirm(true)} disabled={isPending}>
                  <X className="h-4 w-4 mr-1" />
                  Rollback
                </Button>
              </div>
            </div>
          )}

          {/* Start new switchover */}
          {!isActive && (
            <div className="space-y-3">
              <div className="space-y-2">
                <label className="text-sm font-medium">Target Site</label>
                <Select value={targetSiteId} onValueChange={setTargetSiteId}>
                  <SelectTrigger>
                    {targetSiteId ? (
                      (() => {
                        const selectedSite = availableSites.find(s => s.id === targetSiteId);
                        return selectedSite ? (
                          <div className="flex items-center gap-2">
                            <div className={`w-2 h-2 rounded-full ${
                              selectedSite.site_type === 'dr' ? 'bg-orange-500' :
                              selectedSite.site_type === 'primary' ? 'bg-green-500' : 'bg-blue-500'
                            }`} />
                            {selectedSite.name} ({selectedSite.code}) - {selectedSite.site_type.toUpperCase()}
                          </div>
                        ) : <SelectValue placeholder="Select target site..." />;
                      })()
                    ) : (
                      <SelectValue placeholder="Select target site..." />
                    )}
                  </SelectTrigger>
                  <SelectContent>
                    {availableSites.map((site) => (
                      <SelectItem key={site.id} value={site.id}>
                        <div className="flex items-center gap-2">
                          <div className={`w-2 h-2 rounded-full ${
                            site.site_type === 'dr' ? 'bg-orange-500' :
                            site.site_type === 'primary' ? 'bg-green-500' : 'bg-blue-500'
                          }`} />
                          {site.name} ({site.code}) - {site.site_type.toUpperCase()}
                        </div>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <label className="text-sm font-medium">Switchover Mode</label>
                <Select value={mode} onValueChange={(v) => setMode(v as 'FULL' | 'SELECTIVE')}>
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="FULL">
                      <div className="flex flex-col">
                        <span>Full Application</span>
                        <span className="text-xs text-muted-foreground">Switch all components</span>
                      </div>
                    </SelectItem>
                    <SelectItem value="SELECTIVE">
                      <div className="flex flex-col">
                        <span>Selective</span>
                        <span className="text-xs text-muted-foreground">Choose specific components</span>
                      </div>
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>

              {/* Component selection for SELECTIVE mode */}
              {mode === 'SELECTIVE' && components && components.length > 0 && (
                <div className="space-y-2">
                  <label className="text-sm font-medium">Select Components</label>
                  <div className="max-h-40 overflow-y-auto border rounded-md p-2 space-y-1">
                    {components.map((comp) => (
                      <label key={comp.id} className="flex items-center gap-2 p-1 hover:bg-muted/50 rounded cursor-pointer">
                        <input
                          type="checkbox"
                          checked={selectedIds.includes(comp.id)}
                          onChange={(e) => {
                            if (e.target.checked) {
                              setSelectedIds([...selectedIds, comp.id]);
                            } else {
                              setSelectedIds(selectedIds.filter(id => id !== comp.id));
                            }
                          }}
                          className="rounded"
                        />
                        <span className="text-sm">{comp.name}</span>
                        <span className={`text-xs px-1 rounded ${
                          comp.current_state === 'RUNNING' ? 'bg-green-100 text-green-700' :
                          comp.current_state === 'STOPPED' ? 'bg-gray-100 text-gray-700' :
                          'bg-yellow-100 text-yellow-700'
                        }`}>
                          {comp.current_state}
                        </span>
                      </label>
                    ))}
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {selectedIds.length} component(s) selected
                  </p>
                </div>
              )}

              <Alert>
                <AlertTriangle className="h-4 w-4" />
                <AlertDescription>
                  {mode === 'FULL'
                    ? 'Full switchover will stop all components (reverse DAG order), update all agent bindings, then restart on the target site (DAG order).'
                    : `Selective switchover will stop only the ${selectedIds.length} selected component(s), update their bindings, and restart them on the target site. Other components remain unchanged.`}
                </AlertDescription>
              </Alert>
            </div>
          )}

          {/* Error display */}
          {error && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {isActive ? 'Close' : 'Cancel'}
          </Button>
          {!isActive && (
            <Button onClick={handleStart} disabled={!targetSiteId || isPending}>
              {isPending ? <Loader2 className="h-4 w-4 animate-spin mr-2" /> : null}
              <RefreshCw className="h-4 w-4 mr-2" />
              Start Switchover
            </Button>
          )}
        </DialogFooter>
      </DialogContent>

      {/* Rollback Confirmation Dialog */}
      <AlertDialog open={showRollbackConfirm} onOpenChange={setShowRollbackConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle className="flex items-center gap-2">
              <AlertTriangle className="h-5 w-5 text-destructive" />
              Rollback Switchover
            </AlertDialogTitle>
            <AlertDialogDescription>
              This will cancel the switchover and attempt to restore the application to its previous state.
              Components may need to be manually restarted on the original site.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleRollback} className="bg-destructive text-destructive-foreground hover:bg-destructive/90">
              Rollback
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Commit Confirmation Dialog */}
      <AlertDialog open={showCommitConfirm} onOpenChange={setShowCommitConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle className="flex items-center gap-2">
              <Check className="h-5 w-5 text-green-600" />
              Commit Switchover
            </AlertDialogTitle>
            <AlertDialogDescription>
              This will finalize the site switchover. The application will now be running on the target site.
              This action cannot be undone automatically.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleCommit} className="bg-green-600 text-white hover:bg-green-700">
              Commit Switchover
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Success Dialog */}
      <AlertDialog open={showSuccessDialog} onOpenChange={(open) => {
        setShowSuccessDialog(open);
        if (!open) onClose();
      }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle className="flex items-center gap-2 text-green-600">
              <CheckCircle2 className="h-6 w-6" />
              Switchover Complete
            </AlertDialogTitle>
            <AlertDialogDescription className="space-y-2">
              <p>The application has been successfully switched to the target site.</p>
              <p className="text-sm text-muted-foreground">
                All components are now running with the new site configuration.
              </p>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogAction onClick={() => {
              setShowSuccessDialog(false);
              onClose();
            }}>
              Done
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Dialog>
  );
}
