import { useState, useCallback, useMemo, useEffect } from 'react';
import { useParams, Link } from 'react-router-dom';
import {
  useApp,
  useStartApp,
  useStopApp,
  useCancelOperation,
  useStartBranch,
  useCreateComponent,
  useUpdateComponent,
  useDeleteComponent,
  useAddDependency,
  useDeleteDependency,
  useUpdateComponentPositions,
  useExportAppMutation,
} from '@/api/apps';
import { useStartComponent, useStopComponent, useForceStopComponent, useStartWithDeps, useRestartWithDependents } from '@/api/components';
import { usePermission } from '@/hooks/use-permission';
import { useWebSocket } from '@/hooks/use-websocket';
import {
  AppMap,
  ImpactPreview,
  BranchHighlight,
  EdgeHighlight,
  findDependents,
  findDependencies,
  computeBranchHighlight,
} from '@/components/maps/AppMap';
import { DetailPanel } from '@/components/maps/DetailPanel';
import { ShareModal } from '@/components/share/ShareModal';
import { CommandModal } from '@/components/commands/CommandModal';
import { ActivityPanel } from '@/components/activity/ActivityPanel';
import { ComponentPalette } from '@/components/maps/ComponentPalette';
import { ComponentEditor, ComponentFormData } from '@/components/maps/ComponentEditor';
import { ImpactPreviewDialog } from '@/components/maps/ImpactPreviewDialog';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Pencil, Download, Save, ArrowLeft, Play, Square, Loader2,
  Sun, CloudSun, Cloud, CloudRain, CloudLightning,
} from 'lucide-react';

const weatherIcons: Record<string, React.ComponentType<{ className?: string }>> = {
  sunny: Sun,
  fair: CloudSun,
  cloudy: Cloud,
  rainy: CloudRain,
  stormy: CloudLightning,
};

function WeatherIcon({ weather, className }: { weather: string; className?: string }) {
  const Icon = weatherIcons[weather] || Cloud;
  return <Icon className={className} />;
}

function getWeatherVariant(weather: string) {
  if (weather === 'sunny') return 'running' as const;
  if (weather === 'stormy') return 'failed' as const;
  if (weather === 'rainy') return 'degraded' as const;
  return 'secondary' as const;
}

export function MapViewPage() {
  const { appId } = useParams<{ appId: string }>();
  const { data: app, isLoading } = useApp(appId || '');
  const { canOperate, canEdit } = usePermission(appId || '');
  const startApp = useStartApp();
  const stopApp = useStopApp();
  const cancelOperation = useCancelOperation();
  const startBranch = useStartBranch();
  const startComponent = useStartComponent();
  const stopComponent = useStopComponent();
  const forceStopComponent = useForceStopComponent();
  const startWithDeps = useStartWithDeps();
  const restartWithDependents = useRestartWithDependents();
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
  const [isOperating, setIsOperating] = useState(false);
  const [operationType, setOperationType] = useState<'start' | 'stop' | null>(null);

  // Edit mode state
  const [editMode, setEditMode] = useState(false);
  const [pendingPositions, setPendingPositions] = useState<Map<string, { x: number; y: number }>>(new Map());
  const [editorOpen, setEditorOpen] = useState(false);
  const [editingComponent, setEditingComponent] = useState<string | null>(null);
  const [newComponentType, setNewComponentType] = useState<string | null>(null);
  const [newComponentPosition, setNewComponentPosition] = useState<{ x: number; y: number } | null>(null);

  // Impact preview state (shared between AppMap and DetailPanel)
  const [impactPreview, setImpactPreview] = useState<ImpactPreview | null>(null);

  // Edge highlight state (when clicking an edge)
  const [edgeHighlight, setEdgeHighlight] = useState<EdgeHighlight | null>(null);

  // Subscribe to app events via WebSocket
  useEffect(() => {
    if (appId) {
      subscribe(appId);
    }
  }, [appId, subscribe]);

  const components = app?.components || [];
  const dependencies = app?.dependencies || [];

  // Compute component state counts
  const componentCounts = useMemo(() => {
    const counts = { running: 0, stopped: 0, failed: 0, starting: 0, stopping: 0, other: 0 };
    for (const c of components) {
      switch (c.current_state) {
        case 'RUNNING': counts.running++; break;
        case 'STOPPED': counts.stopped++; break;
        case 'FAILED': counts.failed++; break;
        case 'STARTING': counts.starting++; break;
        case 'STOPPING': counts.stopping++; break;
        default: counts.other++; break;
      }
    }
    return counts;
  }, [components]);

  // Compute global state (weather) from component states
  const globalState = useMemo(() => {
    if (components.length === 0) return 'UNKNOWN';
    if (componentCounts.failed > 0) return 'FAILED';
    if (componentCounts.starting > 0 || componentCounts.stopping > 0) return 'TRANSITIONING';
    if (componentCounts.running === components.length) return 'RUNNING';
    if (componentCounts.stopped === components.length) return 'STOPPED';
    if (componentCounts.running > 0 && componentCounts.stopped > 0) return 'DEGRADED';
    return 'UNKNOWN';
  }, [components.length, componentCounts]);

  const weather = useMemo(() => {
    if (globalState === 'RUNNING') return 'sunny';
    if (globalState === 'FAILED') return 'stormy';
    if (globalState === 'DEGRADED' || globalState === 'TRANSITIONING') return 'rainy';
    if (globalState === 'STOPPED') return 'cloudy';
    return 'cloudy';
  }, [globalState]);

  const selectedComponent = components.find((c) => c.id === selectedComponentId) || null;
  const editingComponentData = editingComponent
    ? components.find((c) => c.id === editingComponent) || null
    : null;

  // Compute branch highlight based on selection (only when not in impact preview or edge highlight)
  const branchHighlight = useMemo<BranchHighlight | null>(() => {
    if (impactPreview || edgeHighlight || !selectedComponentId || editMode) return null;
    return computeBranchHighlight(selectedComponentId, dependencies);
  }, [selectedComponentId, dependencies, impactPreview, edgeHighlight, editMode]);

  // Handle component selection - clear edge highlight
  const handleSelectComponent = useCallback((id: string | null) => {
    setSelectedComponentId(id);
    setEdgeHighlight(null); // Clear edge highlight when selecting a component
  }, []);

  // Handle edge click - highlight edge and clear component selection
  const handleEdgeClick = useCallback((edgeId: string, sourceId: string, targetId: string) => {
    setEdgeHighlight({ edgeId, sourceId, targetId });
    setSelectedComponentId(null); // Clear component selection when clicking an edge
  }, []);

  // Get impacted component names for the dialog
  const impactedComponents = useMemo(() => {
    if (!impactPreview) return [];
    return components
      .filter(c => impactPreview.impactedIds.has(c.id))
      .map(c => ({ id: c.id, name: c.display_name || c.name, state: c.current_state }));
  }, [impactPreview, components]);

  const handleStartAll = useCallback(() => {
    if (!appId) return;
    setIsOperating(true);
    setOperationType('start');
    startApp.mutate(appId, {
      onSettled: () => {
        // Keep showing for a bit longer so user sees the transition
        setTimeout(() => {
          setIsOperating(false);
          setOperationType(null);
        }, 2000);
      },
    });
  }, [appId, startApp]);

  const handleStopAll = useCallback(() => {
    if (!appId) return;
    setIsOperating(true);
    setOperationType('stop');
    stopApp.mutate(appId, {
      onSettled: () => {
        setTimeout(() => {
          setIsOperating(false);
          setOperationType(null);
        }, 2000);
      },
    });
  }, [appId, stopApp]);

  const handleCancel = useCallback(() => {
    if (!appId) return;
    if (window.confirm('Cancel the current operation and release the lock?')) {
      cancelOperation.mutate(appId, {
        onSuccess: () => {
          setIsOperating(false);
          setOperationType(null);
        },
      });
    }
  }, [appId, cancelOperation]);

  const handleRestartErrorBranch = useCallback(() => {
    if (appId) startBranch.mutate({ appId });
  }, [appId, startBranch]);

  const getComponentName = useCallback((id: string) => {
    const comp = components.find((c) => c.id === id);
    return comp?.display_name || comp?.name || 'this component';
  }, [components]);

  // Show impact preview for start - automatically detect stopped dependencies
  const handleStartWithPreview = useCallback((id: string) => {
    const name = getComponentName(id);
    const comp = components.find(c => c.id === id);
    const isRunning = comp?.current_state === 'RUNNING' || comp?.current_state === 'STARTING' || comp?.current_state === 'DEGRADED';

    // Find all dependencies
    const allDeps = findDependencies(id, dependencies);

    // Filter to only stopped dependencies (need to be started)
    const stoppedDeps = new Set<string>();
    for (const depId of allDeps) {
      const depComp = components.find(c => c.id === depId);
      if (depComp && depComp.current_state !== 'RUNNING' && depComp.current_state !== 'STARTING') {
        stoppedDeps.add(depId);
      }
    }

    // If component is running but has stopped dependencies → restart branch
    if (isRunning && stoppedDeps.size > 0) {
      setImpactPreview({
        action: 'restart_branch',
        componentId: id,
        componentName: name,
        impactedIds: stoppedDeps,
      });
    }
    // If there are stopped dependencies and component is not running → start with deps
    else if (stoppedDeps.size > 0) {
      setImpactPreview({
        action: 'start_with_deps',
        componentId: id,
        componentName: name,
        impactedIds: stoppedDeps,
      });
    }
    // Component is already running with all deps running
    else if (isRunning) {
      setImpactPreview({
        action: 'start',
        componentId: id,
        componentName: name,
        impactedIds: new Set(),
      });
    }
    // All dependencies are running, simple start
    else {
      setImpactPreview({
        action: 'start',
        componentId: id,
        componentName: name,
        impactedIds: new Set(),
      });
    }
  }, [getComponentName, dependencies, components]);

  // Show impact preview for stop - only show running dependents
  const handleStopWithPreview = useCallback((id: string) => {
    const name = getComponentName(id);

    // Find all dependents
    const allDependents = findDependents(id, dependencies);

    // Filter to only running/starting dependents (actually need to be stopped)
    const runningDependents = new Set<string>();
    for (const depId of allDependents) {
      const comp = components.find(c => c.id === depId);
      if (comp && (comp.current_state === 'RUNNING' || comp.current_state === 'STARTING' || comp.current_state === 'DEGRADED')) {
        runningDependents.add(depId);
      }
    }

    setImpactPreview({
      action: 'stop',
      componentId: id,
      componentName: name,
      impactedIds: runningDependents,
    });
  }, [getComponentName, dependencies, components]);

  // Show impact preview for start with deps - only show stopped dependencies
  const handleStartWithDepsPreview = useCallback((id: string) => {
    const name = getComponentName(id);

    // Find all dependencies
    const allDeps = findDependencies(id, dependencies);

    // Filter to only stopped dependencies (need to be started)
    const stoppedDeps = new Set<string>();
    for (const depId of allDeps) {
      const comp = components.find(c => c.id === depId);
      if (comp && comp.current_state !== 'RUNNING' && comp.current_state !== 'STARTING') {
        stoppedDeps.add(depId);
      }
    }

    setImpactPreview({
      action: 'start_with_deps',
      componentId: id,
      componentName: name,
      impactedIds: stoppedDeps,
    });
  }, [getComponentName, dependencies, components]);

  // Show impact preview for restart with dependents (repair mode)
  // This stops dependents, restarts the component, then starts dependents
  const handleRestartWithDependentsPreview = useCallback((id: string) => {
    const name = getComponentName(id);

    // Find all dependents (components that depend on this one)
    const allDependents = findDependents(id, dependencies);

    // Show all dependents that will be affected
    setImpactPreview({
      action: 'restart_with_dependents',
      componentId: id,
      componentName: name,
      impactedIds: allDependents,
    });
  }, [getComponentName, dependencies]);

  // Execute the action after user confirms
  const handleConfirmAction = useCallback(async () => {
    if (!impactPreview) return;

    switch (impactPreview.action) {
      case 'start':
        startComponent.mutate(impactPreview.componentId);
        break;
      case 'stop':
        stopComponent.mutate(impactPreview.componentId);
        break;
      case 'start_with_deps':
        startWithDeps.mutate(impactPreview.componentId);
        break;
      case 'restart_branch':
        // First stop the component, then start with deps
        // The stop will cascade to dependents, then start_with_deps will rebuild
        try {
          await stopComponent.mutateAsync(impactPreview.componentId);
          // Small delay to let the stop complete
          await new Promise(resolve => setTimeout(resolve, 500));
          startWithDeps.mutate(impactPreview.componentId);
        } catch (error) {
          console.error('Failed to restart branch:', error);
        }
        break;
      case 'restart_with_dependents':
        // Repair mode: stop dependents, stop component, start component, start dependents
        restartWithDependents.mutate(impactPreview.componentId);
        break;
    }

    setImpactPreview(null);
  }, [impactPreview, startComponent, stopComponent, startWithDeps, restartWithDependents]);

  const handleCancelAction = useCallback(() => {
    setImpactPreview(null);
  }, []);

  // Force stop bypasses preview
  const handleForceStopComponent = useCallback((id: string) => {
    const name = getComponentName(id);
    if (window.confirm(`FORCE KILL "${name}"?\n\nThis will stop ONLY this component, ignoring dependencies.\nUse this only in emergencies.`)) {
      forceStopComponent.mutate(id);
    }
  }, [forceStopComponent, getComponentName]);

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
    const comp = components.find((c) => c.id === nodeId);
    if (comp && window.confirm(`Delete component "${comp.name}"?`)) {
      deleteComponent.mutate({ id: nodeId, app_id: appId });
    }
  }, [appId, components, deleteComponent]);

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

  const appName = app?.name || 'app';
  const handleExport = useCallback(async () => {
    if (!appId) return;
    try {
      const data = await exportApp.mutateAsync(appId);
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${appName}-export.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (error) {
      console.error('Export failed:', error);
    }
  }, [appId, appName, exportApp]);

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
        <div className="absolute top-0 left-0 right-0 p-4 z-10 pointer-events-none">
          <div className="pointer-events-auto bg-card/95 backdrop-blur border border-border rounded-lg px-4 py-3 shadow-sm flex items-center justify-between gap-4">
            {/* Left: Back + App name + Weather */}
            <div className="flex items-center gap-3 min-w-0">
              <Link to="/apps" className="p-1 hover:bg-accent rounded shrink-0">
                <ArrowLeft className="h-5 w-5" />
              </Link>
              <WeatherIcon weather={weather} className="h-6 w-6 shrink-0" />
              <h1 className="text-lg font-semibold truncate">{app.name}</h1>
              <Badge variant={getWeatherVariant(weather)} className="shrink-0">
                {globalState}
              </Badge>
              {editMode && (
                <span className="text-xs bg-amber-100 dark:bg-amber-900 text-amber-800 dark:text-amber-200 px-2 py-1 rounded shrink-0">
                  Edit Mode
                </span>
              )}
            </div>

            {/* Center: Component counts */}
            <div className="hidden md:flex items-center gap-4 text-sm">
              {componentCounts.running > 0 && (
                <span className="text-green-600 font-medium">
                  {componentCounts.running} running
                </span>
              )}
              {componentCounts.starting > 0 && (
                <span className="text-blue-600 font-medium animate-pulse">
                  {componentCounts.starting} starting
                </span>
              )}
              {componentCounts.stopping > 0 && (
                <span className="text-blue-600 font-medium animate-pulse">
                  {componentCounts.stopping} stopping
                </span>
              )}
              {componentCounts.stopped > 0 && (
                <span className="text-gray-500">
                  {componentCounts.stopped} stopped
                </span>
              )}
              {componentCounts.failed > 0 && (
                <span className="text-red-600 font-medium">
                  {componentCounts.failed} failed
                </span>
              )}
            </div>

            {/* Right: Actions */}
            <div className="flex items-center gap-2 shrink-0">
              {/* Start/Stop buttons */}
              {canOperate && !editMode && (
                <>
                  {isOperating ? (
                    <div className="flex items-center gap-2">
                      <div className="flex items-center gap-2 text-sm text-muted-foreground px-2">
                        <Loader2 className="h-4 w-4 animate-spin" />
                        <span>{operationType === 'start' ? 'Starting...' : 'Stopping...'}</span>
                      </div>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={handleCancel}
                        className="text-red-600 border-red-300 hover:bg-red-50"
                        title="Cancel operation and release lock"
                      >
                        Cancel
                      </Button>
                    </div>
                  ) : (
                    <>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={handleStartAll}
                        disabled={startApp.isPending || stopApp.isPending}
                        title="Start all components"
                        className="gap-1"
                      >
                        <Play className="h-4 w-4 text-green-600" />
                        <span className="hidden sm:inline">Start All</span>
                      </Button>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={handleStopAll}
                        disabled={startApp.isPending || stopApp.isPending}
                        title="Stop all components"
                        className="gap-1"
                      >
                        <Square className="h-4 w-4 text-red-600" />
                        <span className="hidden sm:inline">Stop All</span>
                      </Button>
                    </>
                  )}
                </>
              )}

              <div className="h-6 w-px bg-border mx-1" />

              <Button
                variant="outline"
                size="sm"
                onClick={handleExport}
                disabled={exportApp.isPending}
                title="Export as JSON"
              >
                <Download className="h-4 w-4" />
                <span className="hidden sm:inline ml-1">Export</span>
              </Button>

              {canEdit && (
                <Button
                  variant={editMode ? 'default' : 'outline'}
                  size="sm"
                  onClick={handleToggleEditMode}
                >
                  {editMode ? (
                    <>
                      <Save className="h-4 w-4" />
                      <span className="hidden sm:inline ml-1">Done</span>
                    </>
                  ) : (
                    <>
                      <Pencil className="h-4 w-4" />
                      <span className="hidden sm:inline ml-1">Edit</span>
                    </>
                  )}
                </Button>
              )}
            </div>
          </div>
        </div>

        {/* Branch highlight legend */}
        {branchHighlight && !editMode && (
          <div className="absolute top-16 left-4 z-10 bg-card/90 backdrop-blur border border-border rounded-md px-3 py-2 text-xs space-y-1">
            <div className="flex items-center gap-2">
              <div className="w-3 h-3 rounded-full bg-indigo-500" />
              <span>Selected</span>
            </div>
            {branchHighlight.dependencyIds.size > 0 && (
              <div className="flex items-center gap-2">
                <div className="w-3 h-3 rounded-full bg-emerald-500" />
                <span>Dependencies ({branchHighlight.dependencyIds.size})</span>
              </div>
            )}
            {branchHighlight.dependentIds.size > 0 && (
              <div className="flex items-center gap-2">
                <div className="w-3 h-3 rounded-full bg-amber-500" />
                <span>Dependents ({branchHighlight.dependentIds.size})</span>
              </div>
            )}
          </div>
        )}

        <AppMap
          components={components}
          dependencies={dependencies}
          selectedComponentId={selectedComponentId}
          onSelectComponent={handleSelectComponent}
          onStartAll={handleStartAll}
          onStopAll={handleStopAll}
          onRestartErrorBranch={handleRestartErrorBranch}
          onShare={() => setShareOpen(true)}
          onToggleActivity={handleToggleActivity}
          activityOpen={activityOpen}
          onStartComponent={handleStartWithPreview}
          onStopComponent={handleStopWithPreview}
          onRestartComponent={handleStartWithPreview}
          onDiagnoseComponent={(id) => handleCommand(id)}
          onForceStopComponent={handleForceStopComponent}
          onStartWithDepsComponent={handleStartWithDepsPreview}
          onRepairComponent={handleRestartWithDependentsPreview}
          canOperate={canOperate}
          // Edit mode props
          editable={editMode}
          onNodePositionChange={handleNodePositionChange}
          onConnect={handleConnect}
          onDeleteEdge={handleDeleteEdge}
          onDeleteNode={handleDeleteNode}
          onNodeDoubleClick={handleNodeDoubleClick}
          onDrop={handleDrop}
          // Highlighting
          impactPreview={impactPreview}
          branchHighlight={branchHighlight}
          edgeHighlight={edgeHighlight}
          onEdgeClick={handleEdgeClick}
        />
      </div>

      {selectedComponent && !editMode && (
        <DetailPanel
          component={selectedComponent}
          onClose={() => setSelectedComponentId(null)}
          onStart={() => handleStartWithPreview(selectedComponent.id)}
          onStop={() => handleStopWithPreview(selectedComponent.id)}
          onRestart={() => handleStartWithPreview(selectedComponent.id)}
          onCommand={() => handleCommand(selectedComponent.id)}
          onDiagnose={() => handleCommand(selectedComponent.id)}
          onForceStop={() => handleForceStopComponent(selectedComponent.id)}
          onStartWithDeps={() => handleStartWithDepsPreview(selectedComponent.id)}
          onRepair={() => handleRestartWithDependentsPreview(selectedComponent.id)}
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

      {/* Impact Preview Dialog */}
      <ImpactPreviewDialog
        open={impactPreview !== null}
        onClose={handleCancelAction}
        onConfirm={handleConfirmAction}
        action={impactPreview?.action || 'start'}
        componentName={impactPreview?.componentName || ''}
        impactedComponents={impactedComponents}
      />
    </div>
  );
}
