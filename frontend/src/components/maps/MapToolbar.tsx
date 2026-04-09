import { useState, useCallback } from 'react';
import { useReactFlow } from '@xyflow/react';
import { Button } from '@/components/ui/button';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';
import {
  ZoomIn, ZoomOut, Maximize, Play, Square,
  GitBranch, Share2, Activity, LayoutGrid, Save, Loader2, ArrowRightLeft,
  FolderOpen, Filter,
} from 'lucide-react';
import { ComponentGroup } from '@/api/apps';
import { GroupPanel } from './GroupPanel';

interface MapToolbarProps {
  onStartAll?: () => void;
  onStopAll?: () => void;
  onRestartErrorBranch?: () => void;
  onSwitchover?: () => void;
  onShare?: () => void;
  onToggleActivity?: () => void;
  activityOpen?: boolean;
  canOperate?: boolean;
  canManage?: boolean;
  canEdit?: boolean;
  onAutoLayout?: () => void;
  onSaveLayout?: () => void;
  hasUnsavedPositions?: boolean;
  isSavingLayout?: boolean;
  // Group management
  groups?: ComponentGroup[];
  components?: Array<{ id: string; group_id?: string | null }>;
  onCreateGroup?: (name: string, color: string, description?: string) => Promise<void>;
  onUpdateGroup?: (groupId: string, name: string, color: string) => Promise<void>;
  onDeleteGroup?: (groupId: string) => Promise<void>;
  // Group filtering
  activeGroupFilter?: string | null;
  onGroupFilterChange?: (groupId: string | null) => void;
}

export function MapToolbar({ onStartAll, onStopAll, onRestartErrorBranch, onSwitchover, onShare, onToggleActivity, activityOpen, canOperate, canManage, canEdit, onAutoLayout, onSaveLayout, hasUnsavedPositions, isSavingLayout, groups, components, onCreateGroup, onUpdateGroup, onDeleteGroup, activeGroupFilter, onGroupFilterChange }: MapToolbarProps) {
  const { zoomIn, zoomOut, fitView } = useReactFlow();
  const [groupPanelOpen, setGroupPanelOpen] = useState(false);

  const handleFit = useCallback(() => fitView({ padding: 0.2 }), [fitView]);

  return (
    <div className="absolute top-24 left-4 z-10 flex gap-2">
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
        <Button
          variant="ghost"
          size="icon"
          className={`h-8 w-8 ${hasUnsavedPositions ? 'text-primary' : 'text-muted-foreground'}`}
          onClick={onSaveLayout}
          disabled={!onSaveLayout || !hasUnsavedPositions || isSavingLayout}
          title={onSaveLayout ? "Save Layout" : "Save Layout (requires edit permission)"}
        >
          {isSavingLayout ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Save className="h-4 w-4" />
          )}
        </Button>
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
          {canManage && (
            <Button variant="ghost" size="icon" className="h-8 w-8" onClick={onSwitchover} title="Site Switchover (DR Failover)">
              <ArrowRightLeft className="h-4 w-4 text-purple-600" />
            </Button>
          )}
        </div>
      )}

      <div className="flex gap-1 bg-card border border-border rounded-md p-1 shadow-sm">
        {/* Group filter buttons */}
        {groups && groups.length > 0 && (
          <>
            <Button
              variant="ghost"
              size="icon"
              className={`h-8 w-8 ${activeGroupFilter === null ? '' : 'bg-primary/10 text-primary'}`}
              onClick={() => onGroupFilterChange?.(null)}
              title={activeGroupFilter ? 'Clear filter' : 'No filter active'}
            >
              <Filter className="h-4 w-4" />
            </Button>
            {groups.map((g) => (
              <Button
                key={g.id}
                variant="ghost"
                size="icon"
                className={`h-8 w-8 ${activeGroupFilter === g.id ? 'ring-2 ring-offset-1' : ''}`}
                onClick={() => onGroupFilterChange?.(activeGroupFilter === g.id ? null : g.id)}
                title={`Filter: ${g.name}`}
              >
                <span
                  className="h-4 w-4 rounded-sm border"
                  style={{ backgroundColor: g.color || '#6366F1' }}
                />
              </Button>
            ))}
          </>
        )}

        {/* Group management popover */}
        {canEdit && onCreateGroup && (
          <Popover open={groupPanelOpen} onOpenChange={setGroupPanelOpen}>
            <PopoverTrigger asChild>
              <Button variant="ghost" size="icon" className="h-8 w-8" title="Manage Groups">
                <FolderOpen className="h-4 w-4" />
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-auto p-0" align="start" side="bottom">
              <GroupPanel
                groups={groups || []}
                components={components || []}
                onCreateGroup={onCreateGroup}
                onUpdateGroup={onUpdateGroup || (async () => {})}
                onDeleteGroup={onDeleteGroup || (async () => {})}
              />
            </PopoverContent>
          </Popover>
        )}

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
