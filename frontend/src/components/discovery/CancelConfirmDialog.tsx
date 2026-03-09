import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { AlertTriangle } from 'lucide-react';
import { useDiscoveryStore } from '@/stores/discovery';

export function CancelConfirmDialog() {
  const { cancelConfirmOpen, setCancelConfirmOpen, cancelDiscovery, phase } = useDiscoveryStore();

  const handleConfirm = () => {
    cancelDiscovery();
  };

  const handleCancel = () => {
    setCancelConfirmOpen(false);
  };

  const getMessage = () => {
    if (phase === 'topology') {
      return 'This will discard all topology selections, edits, and dependencies. You will return to the agent selection phase.';
    }
    return 'This will reset the discovery process. You can start again at any time.';
  };

  return (
    <Dialog open={cancelConfirmOpen} onOpenChange={setCancelConfirmOpen}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-full bg-amber-100 dark:bg-amber-900/30">
              <AlertTriangle className="h-5 w-5 text-amber-600 dark:text-amber-500" />
            </div>
            <div>
              <DialogTitle>Cancel Discovery?</DialogTitle>
              <DialogDescription className="mt-1">
                {getMessage()}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>
        <DialogFooter className="mt-4">
          <Button variant="outline" onClick={handleCancel}>
            Continue Working
          </Button>
          <Button variant="destructive" onClick={handleConfirm}>
            Cancel Discovery
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
