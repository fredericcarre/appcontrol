import { useState, useCallback, useMemo, useEffect } from 'react';
import { useParams, Link, useNavigate } from 'react-router-dom';
import { ConfirmDialog } from '@/components/ui/confirm-dialog';
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
  useDeleteApp,
  useSuspendApp,
  useResumeApp,
} from '@/api/apps';
import { useStartComponent, useStopComponent, useForceStopComponent, useStartWithDeps, useRestartWithDependents } from '@/api/components';
import { usePermission } from '@/hooks/use-permission';
import { useWebSocket } from '@/hooks/use-websocket';
import { useSiteBindings } from '@/api/site-overrides';
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
import { SchedulePanel } from '@/components/schedules/SchedulePanel';
import { HistoryTimeline } from '@/components/history/HistoryTimeline';
import { TimeSnapshot } from '@/api/apps';
import { ComponentPalette } from '@/components/maps/ComponentPalette';
import { ComponentEditor, ComponentFormData } from '@/components/maps/ComponentEditor';
import { ImpactPreviewDialog } from '@/components/maps/ImpactPreviewDialog';
import { SwitchoverPanel } from '@/components/maps/SwitchoverPanel';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Pencil, Download, Save, ArrowLeft, Play, Square, Loader2,
  Sun, CloudSun, Cloud, CloudRain, CloudLightning,
  MoreVertical, Trash2, Pause, PlayCircle, Maximize, Minimize,
  Monitor, History, X, Calendar,
} from 'lucide-react';
import { useFullscreen } from '@/hooks/use-fullscreen';

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
  const navigate = useNavigate();
  const { data: app, isLoading } = useApp(appId || '');
  const { canOperate, canEdit, canManage } = usePermission(appId || '');
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
  const deleteApp = useDeleteApp();
  const suspendApp = useSuspendApp();
  const resumeApp = useResumeApp();
  const { subscribe } = useWebSocket();
  const { data: siteBindingsData } = useSiteBindings(appId || '');

  const [selectedComponentId, setSelectedComponentId] = useState<string | null>(null);
  const [shareOpen, setShareOpen] = useState(false);
  const [commandOpen, setCommandOpen] = useState(false);
  const [commandComponentId, setCommandComponentId] = useState<string | null>(null);
  const [activityOpen, setActivityOpen] = useState(false);
  const [schedulesOpen, setSchedulesOpen] = useState(false);
  const [switchoverOpen, setSwitchoverOpen] = useState(false);
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

  // Confirm dialog state
  const [confirmDialog, setConfirmDialog] = useState<{
    open: boolean;
    title: string;
    description: string;
    confirmLabel: string;
    variant: 'default' | 'destructive' | 'warning';
    onConfirm: () => void;
  }>({
    open: false,
    title: '',
    description: '',
    confirmLabel: 'Confirm',
    variant: 'default',
    onConfirm: () => {},
  });

  // Edge highlight state (when clicking an edge)
  const [edgeHighlight, setEdgeHighlight] = useState<EdgeHighlight | null>(null);

  // Fullscreen state
  const { isFullscreen, toggleFullscreen } = useFullscreen();

  // History mode state
  const [historyMode, setHistoryMode] = useState(false);
  const [historyTime, setHistoryTime] = useState<Date | null>(null);
  const [historySnapshot, setHistorySnapshot] = useState<TimeSnapshot | null>(null);

  // Subscribe to app events via WebSocket
  useEffect(() => {
    if (appId) {
      subscribe(appId);
    }
  }, [appId, subscribe]);

  const liveComponents = useMemo(() => app?.components || [], [app?.components]);
  const dependencies = useMemo(() => app?.dependencies || [], [app?.dependencies]);

  // In history mode, overlay historical states onto components
  const components = useMemo(() => {
    if (!historyMode || !historySnapshot) {
      return liveComponents;
    }
    // Create a map of historical states
    const historicalStates = new Map<string, string>();
    for (const snap of historySnapshot.components) {
      historicalStates.set(snap.id, snap.state);
    }
    // Overlay historical states onto live components
    return liveComponents.map((c) => ({
      ...c,
      current_state: historicalStates.get(c.id) ?? c.current_state,
    }));
  }, [liveComponents, historyMode, historySnapshot]);

  // Compute component state counts
  const componentCounts = useMemo(() => {
    const counts = {
      running: 0,
      degraded: 0,
      stopped: 0,
      failed: 0,
      starting: 0,
      stopping: 0,
      unreachable: 0,
      unknown: 0,
    };
    for (const c of components) {
      switch (c.current_state?.toUpperCase()) {
        case 'RUNNING': counts.running++; break;
        case 'DEGRADED': counts.degraded++; break;
        case 'STOPPED': counts.stopped++; break;
        case 'FAILED': counts.failed++; break;
        case 'STARTING': counts.starting++; break;
        case 'STOPPING': counts.stopping++; break;
        case 'UNREACHABLE': counts.unreachable++; break;
        default: counts.unknown++; break;
      }
    }
    return counts;
  }, [components]);

  // Compute global state (weather) from component states
  // Priority: TRANSITIONING (any start/stop in progress) > FAILED > DEGRADED > RUNNING > STOPPED > UNKNOWN
  const globalState = useMemo(() => {
    if (components.length === 0) return 'UNKNOWN';

    // Any operation in progress = TRANSITIONING (highest priority for UI feedback)
    if (componentCounts.starting > 0 || componentCounts.stopping > 0) return 'TRANSITIONING';

    // Failures take priority over degraded/running
    if (componentCounts.failed > 0) return 'FAILED';

    // Unreachable components are also problematic
    if (componentCounts.unreachable > 0) return 'DEGRADED';

    // Components in DEGRADED state (running but unhealthy)
    if (componentCounts.degraded > 0) return 'DEGRADED';

    // All running (including optional degraded counted above)
    const functionallyRunning = componentCounts.running + componentCounts.degraded;
    if (functionallyRunning === components.length) return 'RUNNING';

    // All stopped
    if (componentCounts.stopped === components.length) return 'STOPPED';

    // Mix of running and stopped
    if ((functionallyRunning > 0 || componentCounts.running > 0) && componentCounts.stopped > 0) return 'DEGRADED';

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
    setConfirmDialog({
      open: true,
      title: 'Start All Components',
      description: `Start all ${components.length} components in "${app?.name || 'this application'}"? This will start components in dependency order.`,
      confirmLabel: 'Start All',
      variant: 'default',
      onConfirm: () => {
        setIsOperating(true);
        setOperationType('start');
        startApp.mutate(appId, {
          onSettled: () => {
            // Clear local state immediately - UI will continue showing transitional
            // state based on globalState === 'TRANSITIONING' from actual component states
            setIsOperating(false);
            setOperationType(null);
          },
        });
      },
    });
  }, [appId, startApp, components.length, app?.name]);

  const handleStopAll = useCallback(() => {
    if (!appId) return;
    setConfirmDialog({
      open: true,
      title: 'Stop All Components',
      description: `Stop all ${components.length} components in "${app?.name || 'this application'}"? This will stop components in reverse dependency order.`,
      confirmLabel: 'Stop All',
      variant: 'warning',
      onConfirm: () => {
        setIsOperating(true);
        setOperationType('stop');
        stopApp.mutate(appId, {
          onSettled: () => {
            setIsOperating(false);
            setOperationType(null);
          },
        });
      },
    });
  }, [appId, stopApp, components.length, app?.name]);

  const handleCancel = useCallback(() => {
    if (!appId) return;
    setConfirmDialog({
      open: true,
      title: 'Cancel Operation',
      description: 'Cancel the current operation and release the lock?',
      confirmLabel: 'Cancel Operation',
      variant: 'warning',
      onConfirm: () => {
        cancelOperation.mutate(appId, {
          onSuccess: () => {
            setIsOperating(false);
            setOperationType(null);
          },
        });
      },
    });
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
    setConfirmDialog({
      open: true,
      title: 'Force Kill Component',
      description: `FORCE KILL "${name}"?\n\nThis will stop ONLY this component, ignoring dependencies.\nUse this only in emergencies.`,
      confirmLabel: 'Force Kill',
      variant: 'destructive',
      onConfirm: () => {
        forceStopComponent.mutate(id);
      },
    });
  }, [forceStopComponent, getComponentName]);

  const handleCommand = useCallback((componentId: string) => {
    setCommandComponentId(componentId);
    setCommandOpen(true);
  }, []);

  const handleToggleActivity = useCallback(() => {
    setActivityOpen((prev) => !prev);
    if (!activityOpen) setSchedulesOpen(false); // Close schedules when opening activity
  }, [activityOpen]);

  const handleToggleSchedules = useCallback(() => {
    setSchedulesOpen((prev) => !prev);
    if (!schedulesOpen) setActivityOpen(false); // Close activity when opening schedules
  }, [schedulesOpen]);

  const handleSwitchover = useCallback(() => {
    setSwitchoverOpen(true);
  }, []);

  const handleActivitySelectComponent = useCallback((componentId: string) => {
    setSelectedComponentId(componentId);
  }, []);

  // Navigate to a referenced app (for application-type components)
  const handleNavigateToApp = useCallback((targetAppId: string) => {
    navigate(`/apps/${targetAppId}`);
  }, [navigate]);

  // History mode handlers
  const handleToggleHistoryMode = useCallback(() => {
    setHistoryMode((prev) => {
      if (prev) {
        // Exiting history mode - clear historical state
        setHistoryTime(null);
        setHistorySnapshot(null);
      }
      return !prev;
    });
  }, []);

  const handleHistoryTimeSelect = useCallback((time: Date, snapshot: TimeSnapshot | null) => {
    setHistoryTime(time);
    setHistorySnapshot(snapshot);
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

  // Handler for saving layout positions from view mode
  const handleSaveLayoutPositions = useCallback((positions: Array<{ id: string; x: number; y: number }>) => {
    updatePositions.mutate(positions);
  }, [updatePositions]);

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
    setConfirmDialog({
      open: true,
      title: 'Delete Dependency',
      description: 'Delete this dependency?',
      confirmLabel: 'Delete',
      variant: 'destructive',
      onConfirm: () => {
        deleteDependency.mutate({ app_id: appId, dependency_id: edgeId });
      },
    });
  }, [appId, deleteDependency]);

  const handleDeleteNode = useCallback((nodeId: string) => {
    if (!appId) return;
    const comp = components.find((c) => c.id === nodeId);
    if (!comp) return;
    setConfirmDialog({
      open: true,
      title: 'Delete Component',
      description: `Delete component "${comp.name}"?`,
      confirmLabel: 'Delete',
      variant: 'destructive',
      onConfirm: () => {
        deleteComponent.mutate({ id: nodeId, app_id: appId });
      },
    });
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
        display_name: data.display_name || null,
        description: data.description || null,
        component_type: data.component_type,
        icon: data.icon,
        host: data.host || null,
        group_id: data.group_id,
        check_cmd: data.check_cmd || null,
        start_cmd: data.start_cmd || null,
        stop_cmd: data.stop_cmd || null,
        // Timeouts and intervals
        check_interval_seconds: data.check_interval_seconds,
        start_timeout_seconds: data.start_timeout_seconds,
        stop_timeout_seconds: data.stop_timeout_seconds,
        is_optional: data.is_optional,
        // Application reference
        referenced_app_id: data.referenced_app_id || null,
        // Cluster configuration
        cluster_size: data.cluster_size ?? null,
        cluster_nodes: data.cluster_nodes?.length ? data.cluster_nodes : null,
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
        // Timeouts and intervals
        check_interval_seconds: data.check_interval_seconds,
        start_timeout_seconds: data.start_timeout_seconds,
        stop_timeout_seconds: data.stop_timeout_seconds,
        is_optional: data.is_optional,
        // Application reference (for app-type components)
        referenced_app_id: data.referenced_app_id || undefined,
        // Cluster configuration
        cluster_size: data.cluster_size ?? undefined,
        cluster_nodes: data.cluster_nodes?.length ? data.cluster_nodes : undefined,
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

  const handleDeleteApp = useCallback(() => {
    if (!appId || !app) return;
    setConfirmDialog({
      open: true,
      title: 'Delete Application',
      description: `Are you sure you want to delete "${app.name}"? This action cannot be undone.`,
      confirmLabel: 'Delete',
      variant: 'destructive',
      onConfirm: async () => {
        try {
          await deleteApp.mutateAsync(appId);
          navigate('/apps');
        } catch (error) {
          console.error('Delete failed:', error);
        }
      },
    });
  }, [appId, app, deleteApp, navigate]);

  const handleSuspendApp = useCallback(async () => {
    if (!appId) return;
    try {
      await suspendApp.mutateAsync(appId);
    } catch (error) {
      console.error('Suspend failed:', error);
    }
  }, [appId, suspendApp]);

  const handleResumeApp = useCallback(async () => {
    if (!appId) return;
    try {
      await resumeApp.mutateAsync(appId);
    } catch (error) {
      console.error('Resume failed:', error);
    }
  }, [appId, resumeApp]);

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
              {historyMode && (
                <Badge variant="secondary" className="shrink-0 bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-200">
                  <History className="h-3 w-3 mr-1" />
                  {historyTime ? new Date(historyTime).toLocaleString() : 'Historical View'}
                </Badge>
              )}
              {app.is_suspended && (
                <Badge variant="secondary" className="shrink-0 bg-orange-100 text-orange-800 dark:bg-orange-900 dark:text-orange-200">
                  <Pause className="h-3 w-3 mr-1" />
                  Suspended
                </Badge>
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
              {/* Start/Stop buttons - hidden in history mode */}
              {canOperate && !editMode && !historyMode && (
                <>
                  {(isOperating || globalState === 'TRANSITIONING') ? (
                    <div className="flex items-center gap-2">
                      <div className="flex items-center gap-2 text-sm text-muted-foreground px-2">
                        <Loader2 className="h-4 w-4 animate-spin" />
                        <span>
                          {isOperating
                            ? (operationType === 'start' ? 'Starting...' : 'Stopping...')
                            : (componentCounts.starting > 0 ? 'Starting...' : 'Stopping...')}
                        </span>
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

              {/* Edit mode button - always visible when user can edit, hidden in history mode */}
              {canEdit && !historyMode && (
                <>
                  <div className="h-6 w-px bg-border mx-1" />
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
                </>
              )}

              {/* History mode toggle */}
              {!editMode && (
                <>
                  <div className="h-6 w-px bg-border mx-1" />
                  <Button
                    variant={historyMode ? 'default' : 'outline'}
                    size="sm"
                    onClick={handleToggleHistoryMode}
                    className={historyMode ? 'bg-purple-600 hover:bg-purple-700' : ''}
                    title={historyMode ? 'Exit History Mode' : 'View History'}
                  >
                    {historyMode ? (
                      <>
                        <X className="h-4 w-4" />
                        <span className="hidden sm:inline ml-1">Exit History</span>
                      </>
                    ) : (
                      <>
                        <History className="h-4 w-4" />
                        <span className="hidden sm:inline ml-1">History</span>
                      </>
                    )}
                  </Button>
                </>
              )}

              {/* More actions dropdown - contains less critical actions */}
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="outline" size="sm">
                    <MoreVertical className="h-4 w-4" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onClick={handleExport} disabled={exportApp.isPending}>
                    <Download className="h-4 w-4 mr-2" />
                    Export as JSON
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={toggleFullscreen}>
                    {isFullscreen ? (
                      <Minimize className="h-4 w-4 mr-2" />
                    ) : (
                      <Maximize className="h-4 w-4 mr-2" />
                    )}
                    {isFullscreen ? 'Exit Fullscreen' : 'Fullscreen'}
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => navigate('/supervision')}>
                    <Monitor className="h-4 w-4 mr-2" />
                    Supervision Mode
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onClick={handleToggleSchedules}>
                    <Calendar className="h-4 w-4 mr-2" />
                    {schedulesOpen ? 'Hide Schedules' : 'Schedules'}
                  </DropdownMenuItem>
                  {canManage && (
                    <>
                      <DropdownMenuSeparator />
                      {app.is_suspended ? (
                        <DropdownMenuItem
                          onClick={handleResumeApp}
                          disabled={resumeApp.isPending}
                        >
                          <PlayCircle className="h-4 w-4 mr-2 text-green-600" />
                          Resume Checks
                        </DropdownMenuItem>
                      ) : (
                        <DropdownMenuItem
                          onClick={handleSuspendApp}
                          disabled={suspendApp.isPending}
                        >
                          <Pause className="h-4 w-4 mr-2 text-orange-600" />
                          Suspend Checks
                        </DropdownMenuItem>
                      )}
                      <DropdownMenuSeparator />
                      <DropdownMenuItem
                        onClick={handleDeleteApp}
                        disabled={deleteApp.isPending}
                        className="text-red-600 focus:text-red-600"
                      >
                        <Trash2 className="h-4 w-4 mr-2" />
                        Delete Application
                      </DropdownMenuItem>
                    </>
                  )}
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>
        </div>

        {/* Branch highlight legend */}
        {branchHighlight && !editMode && (
          <div className="absolute top-36 left-4 z-10 bg-card/90 backdrop-blur border border-border rounded-md px-3 py-2 text-xs space-y-1">
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

        {/* Map container - shrink when history mode is active */}
        <div className={historyMode ? 'h-[calc(100%-140px)]' : 'h-full'}>
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
            onSwitchover={handleSwitchover}
            canManage={canManage}
            onStartComponent={historyMode ? undefined : handleStartWithPreview}
            onStopComponent={historyMode ? undefined : handleStopWithPreview}
            onRestartComponent={historyMode ? undefined : handleStartWithPreview}
            onDiagnoseComponent={historyMode ? undefined : (id) => handleCommand(id)}
            onForceStopComponent={historyMode ? undefined : handleForceStopComponent}
            onStartWithDepsComponent={historyMode ? undefined : handleStartWithDepsPreview}
            onRepairComponent={historyMode ? undefined : handleRestartWithDependentsPreview}
            onNavigateToApp={handleNavigateToApp}
            canOperate={canOperate && !historyMode}
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
            // Layout saving (view mode)
            onSaveLayout={canEdit ? handleSaveLayoutPositions : undefined}
            isSavingLayout={updatePositions.isPending}
            // Multi-site data
            componentBindings={siteBindingsData?.component_bindings}
            primarySite={siteBindingsData?.primary_site}
          />
        </div>

        {/* History Timeline - shown at the bottom when in history mode */}
        {historyMode && (
          <HistoryTimeline
            appId={appId || ''}
            onSelectTime={handleHistoryTimeSelect}
          />
        )}
      </div>

      {selectedComponent && !editMode && (
        <DetailPanel
          component={selectedComponent}
          onClose={() => setSelectedComponentId(null)}
          onStart={historyMode ? undefined : () => handleStartWithPreview(selectedComponent.id)}
          onStop={historyMode ? undefined : () => handleStopWithPreview(selectedComponent.id)}
          onRestart={historyMode ? undefined : () => handleStartWithPreview(selectedComponent.id)}
          onCommand={historyMode ? undefined : () => handleCommand(selectedComponent.id)}
          onDiagnose={historyMode ? undefined : () => handleCommand(selectedComponent.id)}
          onForceStop={historyMode ? undefined : () => handleForceStopComponent(selectedComponent.id)}
          onStartWithDeps={historyMode ? undefined : () => handleStartWithDepsPreview(selectedComponent.id)}
          onRepair={historyMode ? undefined : () => handleRestartWithDependentsPreview(selectedComponent.id)}
          canOperate={canOperate && !historyMode}
        />
      )}

      {activityOpen && (
        <ActivityPanel
          appId={appId || ''}
          onClose={() => setActivityOpen(false)}
          onSelectComponent={handleActivitySelectComponent}
        />
      )}

      {schedulesOpen && (
        <SchedulePanel
          appId={appId || ''}
          canOperate={canOperate}
          onClose={() => setSchedulesOpen(false)}
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

      {/* Switchover Panel */}
      <SwitchoverPanel
        appId={appId || ''}
        currentSiteId={app.site_id}
        open={switchoverOpen}
        onClose={() => setSwitchoverOpen(false)}
        components={components.map(c => ({
          id: c.id,
          name: c.display_name || c.name,
          current_state: c.current_state || 'UNKNOWN',
        }))}
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

      {/* Confirm Dialog */}
      <ConfirmDialog
        open={confirmDialog.open}
        onOpenChange={(open) => setConfirmDialog((prev) => ({ ...prev, open }))}
        title={confirmDialog.title}
        description={confirmDialog.description}
        confirmLabel={confirmDialog.confirmLabel}
        variant={confirmDialog.variant}
        onConfirm={confirmDialog.onConfirm}
      />
    </div>
  );
}
