import { useCallback } from 'react';
import { useReactFlow } from '@xyflow/react';
import { Button } from '@/components/ui/button';
import {
  ZoomIn, ZoomOut, Maximize, Play, Square,
  GitBranch, Share2, Activity, LayoutGrid, Save, Loader2,
} from 'lucide-react';

interface MapToolbarProps {
  onStartAll?: () => void;
  onStopAll?: () => void;
  onRestartErrorBranch?: () => void;
  onShare?: () => void;
  onToggleActivity?: () => void;
  activityOpen?: boolean;
  canOperate?: boolean;
  onAutoLayout?: () => void;
  onSaveLayout?: () => void;
  hasUnsavedPositions?: boolean;
  isSavingLayout?: boolean;
}

export function MapToolbar({ onStartAll, onStopAll, onRestartErrorBranch, onShare, onToggleActivity, activityOpen, canOperate, onAutoLayout, onSaveLayout, hasUnsavedPositions, isSavingLayout }: MapToolbarProps) {
  const { zoomIn, zoomOut, fitView } = useReactFlow();

  const handleFit = useCallback(() => fitView({ padding: 0.2 }), [fitView]);

  return (
    <div className="absolute top-4 left-4 z-10 flex gap-2">
      <div className="flex gap-1 bg-card border border-border rounded-md p-1 shadow-sm">
        <Button variant="ghost" size="icon" className="h-8 w-8" onClick={() => zoomIn()}>
          <ZoomIn className="h-4 w-4" />
        </Button>
        <Button variant="ghost" size="icon" className="h-8 w-8" onClick={() => zoomOut()}>
          <ZoomOut className="h-4 w-4" />
        </Button>
        <Button variant="ghost" size="icon" className="h-8 w-8" onClick={handleFit}>
          <Maximize className="h-4 w-4" />
        </Button>
        <Button variant="ghost" size="icon" className="h-8 w-8" onClick={onAutoLayout} title="Auto Layout">
          <LayoutGrid className="h-4 w-4" />
        </Button>
        {onSaveLayout && (
          <Button
            variant="ghost"
            size="icon"
            className={`h-8 w-8 ${hasUnsavedPositions ? 'text-primary' : ''}`}
            onClick={onSaveLayout}
            disabled={!hasUnsavedPositions || isSavingLayout}
            title="Save Layout"
          >
            {isSavingLayout ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Save className="h-4 w-4" />
            )}
          </Button>
        )}
      </div>

      {canOperate && (
        <div className="flex gap-1 bg-card border border-border rounded-md p-1 shadow-sm">
          <Button variant="ghost" size="icon" className="h-8 w-8" onClick={onStartAll} title="Start All">
            <Play className="h-4 w-4 text-green-600" />
          </Button>
          <Button variant="ghost" size="icon" className="h-8 w-8" onClick={onStopAll} title="Stop All">
            <Square className="h-4 w-4 text-red-600" />
          </Button>
          <Button variant="ghost" size="icon" className="h-8 w-8" onClick={onRestartErrorBranch} title="Restart Error Branch">
            <GitBranch className="h-4 w-4 text-orange-600" />
          </Button>
        </div>
      )}

      <div className="flex gap-1 bg-card border border-border rounded-md p-1 shadow-sm">
        <Button variant="ghost" size="icon" className="h-8 w-8" onClick={onShare} title="Share">
          <Share2 className="h-4 w-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className={`h-8 w-8 ${activityOpen ? 'bg-primary/10 text-primary' : ''}`}
          onClick={onToggleActivity}
          title="Activity Feed"
        >
          <Activity className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}
