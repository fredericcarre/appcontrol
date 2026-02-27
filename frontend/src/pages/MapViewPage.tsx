import { useState, useCallback } from 'react';
import { useParams, Link } from 'react-router-dom';
import {
  useApp,
  useStartApp,
  useStopApp,
  useStartBranch,
  useCreateComponent,
  useUpdateComponent,
  useDeleteComponent,
  useAddDependency,
  useDeleteDependency,
  useUpdateComponentPositions,
  useExportAppMutation,
} from '@/api/apps';
import { useStartComponent, useStopComponent, useForceStopComponent, useStartWithDeps } from '@/api/components';
import { usePermission } from '@/hooks/use-permission';
import { useWebSocket } from '@/hooks/use-websocket';
import { AppMap } from '@/components/maps/AppMap';
import { DetailPanel } from '@/components/maps/DetailPanel';
import { ShareModal } from '@/components/share/ShareModal';
import { CommandModal } from '@/components/commands/CommandModal';
import { ActivityPanel } from '@/components/activity/ActivityPanel';
import { ComponentPalette } from '@/components/maps/ComponentPalette';
import { ComponentEditor, ComponentFormData } from '@/components/maps/ComponentEditor';
import { Button } from '@/components/ui/button';
import { Pencil, X, Download, Save, ArrowLeft } from 'lucide-react';

export function MapViewPage() {
  const { appId } = useParams<{ appId: string }>();
  const { data: app, isLoading } = useApp(appId || '');
  const { canOperate, canEdit } = usePermission(appId || '');
  const startApp = useStartApp();
  const stopApp = useStopApp();
  const startBranch = useStartBranch();
  const startComponent = useStartComponent();
  const stopComponent = useStopComponent();
  const forceStopComponent = useForceStopComponent();
  const startWithDeps = useStartWithDeps();
  const createComponent = useCreateComponent();
  const updateComponent = useUpdateComponent();
  const deleteComponent = useDeleteComponent();
  const addDependency = useAddDependency();
  const deleteDependency = useDeleteDependency();
  const updatePositions = useUpdateComponentPositions();
  const exportApp = useExportAppMutation();
  const { subscribe } = useWebSocket();

  const [selectedComponentId, setSelectedComponentId] = useState<string | null>(null);
  const [shareOpen, setShareOpen] = useState(false);
  const [commandOpen, setCommandOpen] = useState(false);
  const [commandComponentId, setCommandComponentId] = useState<string | null>(null);
  const [activityOpen, setActivityOpen] = useState(false);

  // Edit mode state
  const [editMode, setEditMode] = useState(false);
  const [pendingPositions, setPendingPositions] = useState<Map<string, { x: number; y: number }>>(new Map());
  const [editorOpen, setEditorOpen] = useState(false);
  const [editingComponent, setEditingComponent] = useState<string | null>(null);
  const [newComponentType, setNewComponentType] = useState<string | null>(null);
  const [newComponentPosition, setNewComponentPosition] = useState<{ x: number; y: number } | null>(null);

  // Subscribe to app events
  if (appId) {
    subscribe(appId);
  }

  const selectedComponent = app?.components.find((c) => c.id === selectedComponentId) || null;
  const editingComponentData = editingComponent
    ? app?.components.find((c) => c.id === editingComponent) || null
    : null;

  const handleStartAll = useCallback(() => {
    if (appId) startApp.mutate(appId);
  }, [appId, startApp]);

  const handleStopAll = useCallback(() => {
    if (appId) stopApp.mutate(appId);
  }, [appId, stopApp]);

  const handleRestartErrorBranch = useCallback(() => {
    if (appId) startBranch.mutate({ appId });
  }, [appId, startBranch]);

  const handleStartComponent = useCallback((id: string) => {
    startComponent.mutate(id);
  }, [startComponent]);

  const handleStopComponent = useCallback((id: string) => {
    stopComponent.mutate(id);
  }, [stopComponent]);

  const handleForceStopComponent = useCallback((id: string) => {
    if (window.confirm('Force kill this component? This will ignore all dependencies.')) {
      forceStopComponent.mutate(id);
    }
  }, [forceStopComponent]);

  const handleStartWithDeps = useCallback((id: string) => {
    startWithDeps.mutate(id);
  }, [startWithDeps]);

  const handleCommand = useCallback((componentId: string) => {
    setCommandComponentId(componentId);
    setCommandOpen(true);
  }, []);

  const handleToggleActivity = useCallback(() => {
    setActivityOpen((prev) => !prev);
  }, []);

  const handleActivitySelectComponent = useCallback((componentId: string) => {
    setSelectedComponentId(componentId);
  }, []);

  // Edit mode handlers
  const handleToggleEditMode = useCallback(() => {
    if (editMode && pendingPositions.size > 0) {
      // Save pending positions when exiting edit mode
      const positions = Array.from(pendingPositions.entries()).map(([id, pos]) => ({
        id,
        x: pos.x,
        y: pos.y,
      }));
      updatePositions.mutate(positions);
      setPendingPositions(new Map());
    }
    setEditMode((prev) => !prev);
  }, [editMode, pendingPositions, updatePositions]);

  const handleNodePositionChange = useCallback((nodeId: string, x: number, y: number) => {
    setPendingPositions((prev) => {
      const next = new Map(prev);
      next.set(nodeId, { x, y });
      return next;
    });
  }, []);

  const handleConnect = useCallback((sourceId: string, targetId: string) => {
    if (!appId) return;
    addDependency.mutate({
      app_id: appId,
      from_component_id: sourceId,
      to_component_id: targetId,
    });
  }, [appId, addDependency]);

  const handleDeleteEdge = useCallback((edgeId: string) => {
    if (!appId) return;
    if (window.confirm('Delete this dependency?')) {
      deleteDependency.mutate({ app_id: appId, dependency_id: edgeId });
    }
  }, [appId, deleteDependency]);

  const handleDeleteNode = useCallback((nodeId: string) => {
    if (!appId) return;
    const comp = app?.components.find((c) => c.id === nodeId);
    if (comp && window.confirm(`Delete component "${comp.name}"?`)) {
      deleteComponent.mutate({ id: nodeId, app_id: appId });
    }
  }, [appId, app, deleteComponent]);

  const handleNodeDoubleClick = useCallback((nodeId: string) => {
    if (!editMode) return;
    setEditingComponent(nodeId);
    setNewComponentType(null);
    setEditorOpen(true);
  }, [editMode]);

  const handleDrop = useCallback((type: string, position: { x: number; y: number }) => {
    setNewComponentType(type);
    setNewComponentPosition(position);
    setEditingComponent(null);
    setEditorOpen(true);
  }, []);

  const handleEditorSave = useCallback((data: ComponentFormData) => {
    if (!appId) return;

    if (editingComponent) {
      // Update existing component
      updateComponent.mutate({
        id: editingComponent,
        app_id: appId,
        name: data.name,
        display_name: data.display_name || undefined,
        description: data.description || undefined,
        component_type: data.component_type,
        icon: data.icon,
        host: data.host || undefined,
        group_id: data.group_id,
        check_cmd: data.check_cmd || undefined,
        start_cmd: data.start_cmd || undefined,
        stop_cmd: data.stop_cmd || undefined,
      });
    } else if (newComponentType && newComponentPosition) {
      // Create new component
      createComponent.mutate({
        app_id: appId,
        name: data.name,
        display_name: data.display_name || undefined,
        description: data.description || undefined,
        component_type: data.component_type,
        icon: data.icon,
        host: data.host || '',
        group_id: data.group_id || undefined,
        check_cmd: data.check_cmd || undefined,
        start_cmd: data.start_cmd || undefined,
        stop_cmd: data.stop_cmd || undefined,
        position_x: newComponentPosition.x,
        position_y: newComponentPosition.y,
      });
    }

    setEditorOpen(false);
    setEditingComponent(null);
    setNewComponentType(null);
    setNewComponentPosition(null);
  }, [appId, editingComponent, newComponentType, newComponentPosition, createComponent, updateComponent]);

  const handleEditorClose = useCallback(() => {
    setEditorOpen(false);
    setEditingComponent(null);
    setNewComponentType(null);
    setNewComponentPosition(null);
  }, []);

  const handleExport = useCallback(async () => {
    if (!appId) return;
    try {
      const data = await exportApp.mutateAsync(appId);
      // Download as JSON file
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${app?.name || 'app'}-export.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (error) {
      console.error('Export failed:', error);
    }
  }, [appId, app?.name, exportApp]);

  if (isLoading || !app) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="animate-spin h-8 w-8 border-2 border-primary border-t-transparent rounded-full" />
      </div>
    );
  }

  return (
    <div className="h-full flex">
      {/* Left palette (edit mode only) */}
      {editMode && (
        <ComponentPalette className="w-52 flex-shrink-0 m-2 z-10" />
      )}

      <div className="flex-1 relative">
        {/* Header */}
        <div className="absolute top-0 left-0 right-0 p-4 z-10 pointer-events-none flex items-center justify-between">
          <div className="pointer-events-auto flex items-center gap-2">
            <Link to="/apps" className="p-1 hover:bg-accent rounded">
              <ArrowLeft className="h-5 w-5" />
            </Link>
            <h1 className="text-xl font-bold bg-card/80 backdrop-blur px-3 py-1 rounded-md border border-border">
              {app.name}
            </h1>
            {editMode && (
              <span className="text-xs bg-amber-100 dark:bg-amber-900 text-amber-800 dark:text-amber-200 px-2 py-1 rounded">
                Edit Mode
              </span>
            )}
          </div>

          <div className="pointer-events-auto flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleExport}
              disabled={exportApp.isPending}
              title="Export as JSON"
            >
              <Download className="h-4 w-4 mr-1" />
              Export
            </Button>

            {canEdit && (
              <Button
                variant={editMode ? 'default' : 'outline'}
                size="sm"
                onClick={handleToggleEditMode}
              >
                {editMode ? (
                  <>
                    <Save className="h-4 w-4 mr-1" />
                    Done
                  </>
                ) : (
                  <>
                    <Pencil className="h-4 w-4 mr-1" />
                    Edit
                  </>
                )}
              </Button>
            )}
          </div>
        </div>

        <AppMap
          components={app.components || []}
          dependencies={app.dependencies || []}
          onSelectComponent={setSelectedComponentId}
          onStartAll={handleStartAll}
          onStopAll={handleStopAll}
          onRestartErrorBranch={handleRestartErrorBranch}
          onShare={() => setShareOpen(true)}
          onToggleActivity={handleToggleActivity}
          activityOpen={activityOpen}
          onStartComponent={handleStartComponent}
          onStopComponent={handleStopComponent}
          onRestartComponent={handleStartComponent}
          onDiagnoseComponent={(id) => handleCommand(id)}
          onForceStopComponent={handleForceStopComponent}
          onStartWithDepsComponent={handleStartWithDeps}
          canOperate={canOperate}
          // Edit mode props
          editable={editMode}
          onNodePositionChange={handleNodePositionChange}
          onConnect={handleConnect}
          onDeleteEdge={handleDeleteEdge}
          onDeleteNode={handleDeleteNode}
          onNodeDoubleClick={handleNodeDoubleClick}
          onDrop={handleDrop}
        />
      </div>

      {selectedComponent && !editMode && (
        <DetailPanel
          component={selectedComponent}
          onClose={() => setSelectedComponentId(null)}
          onStart={() => handleStartComponent(selectedComponent.id)}
          onStop={() => handleStopComponent(selectedComponent.id)}
          onRestart={() => handleStartComponent(selectedComponent.id)}
          onCommand={() => handleCommand(selectedComponent.id)}
          onDiagnose={() => handleCommand(selectedComponent.id)}
          onForceStop={() => handleForceStopComponent(selectedComponent.id)}
          onStartWithDeps={() => handleStartWithDeps(selectedComponent.id)}
          canOperate={canOperate}
        />
      )}

      {activityOpen && (
        <ActivityPanel
          appId={appId || ''}
          onClose={() => setActivityOpen(false)}
          onSelectComponent={handleActivitySelectComponent}
        />
      )}

      <ShareModal
        appId={appId || ''}
        open={shareOpen}
        onOpenChange={setShareOpen}
      />

      {commandComponentId && (
        <CommandModal
          componentId={commandComponentId}
          open={commandOpen}
          onOpenChange={(open) => {
            setCommandOpen(open);
            if (!open) setCommandComponentId(null);
          }}
        />
      )}

      <ComponentEditor
        component={editingComponentData}
        appId={appId || ''}
        open={editorOpen}
        onClose={handleEditorClose}
        onSave={handleEditorSave}
        isCreating={!editingComponent}
        initialType={newComponentType || undefined}
      />
    </div>
  );
}
