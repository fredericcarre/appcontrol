import { useCallback, useMemo, useRef, useEffect, useState, DragEvent } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  Node,
  Edge,
  useNodesState,
  useEdgesState,
  NodeTypes,
  BackgroundVariant,
  Connection,
  NodeChange,
  EdgeChange,
  applyNodeChanges,
  applyEdgeChanges,
  useReactFlow,
  ReactFlowProvider,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import Dagre from '@dagrejs/dagre';
import { toast } from 'sonner';
import { ComponentNode } from './ComponentNode';
import { MapToolbar } from './MapToolbar';
import { InfrastructureSummary } from './InfrastructureSummary';
import { EdgeToolbar } from './EdgeToolbar';
import { MapContextMenu, MapContextMenuProps } from './MapContextMenu';
import { Component, Dependency, ComponentGroup } from '@/api/apps';
import { SiteBinding, SiteInfo, ComponentSiteBindings, groupBindingsByComponent } from '@/api/site-overrides';
import { ComponentState, ComponentType, STATE_COLORS } from '@/lib/colors';

const nodeTypes: NodeTypes = {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  component: ComponentNode as any,
};

export interface BranchHighlight {
  selectedId: string;
  dependencyIds: Set<string>;  // Components this one depends on (upstream)
  dependentIds: Set<string>;   // Components that depend on this one (downstream)
}

export interface EdgeHighlight {
  edgeId: string;
  sourceId: string;
  targetId: string;
}

export interface ImpactPreview {
  action: 'start' | 'stop' | 'start_with_deps' | 'restart_branch' | 'restart_with_dependents';
  componentId: string;
  componentName: string;
  impactedIds: Set<string>;
}

interface AppMapProps {
  components: Component[];
  dependencies: Dependency[];
  groups?: ComponentGroup[];
  onSelectComponent: (id: string | null) => void;
  selectedComponentId?: string | null;
  onStartAll?: () => void;
  onStopAll?: () => void;
  onRestartErrorBranch?: () => void;
  onShare?: () => void;
  onToggleActivity?: () => void;
  activityOpen?: boolean;
  onSwitchover?: () => void;
  canManage?: boolean;
  onStartComponent?: (id: string) => void;
  onStopComponent?: (id: string) => void;
  onRestartComponent?: (id: string) => void;
  onDiagnoseComponent?: (id: string) => void;
  onForceStopComponent?: (id: string) => void;
  onStartWithDepsComponent?: (id: string) => void;
  onRepairComponent?: (id: string) => void;
  onNavigateToApp?: (appId: string) => void;
  canOperate?: boolean;
  // Edit mode props
  editable?: boolean;
  onNodePositionChange?: (nodeId: string, x: number, y: number) => void;
  onConnect?: (sourceId: string, targetId: string) => void;
  onDeleteEdge?: (edgeId: string) => void;
  onDeleteNode?: (nodeId: string) => void;
  onNodeDoubleClick?: (nodeId: string) => void;
  onDrop?: (type: string, position: { x: number; y: number }) => void;
  // Impact preview (controlled from parent)
  impactPreview?: ImpactPreview | null;
  // Branch highlight (computed from selection)
  branchHighlight?: BranchHighlight | null;
  // Edge highlight (when clicking an edge)
  edgeHighlight?: EdgeHighlight | null;
  onEdgeClick?: (edgeId: string, sourceId: string, targetId: string) => void;
  // Allow dragging nodes in view mode for visualization
  allowDrag?: boolean;
  // Layout saving
  onSaveLayout?: (positions: Array<{ id: string; x: number; y: number }>) => void;
  isSavingLayout?: boolean;
  // Multi-site data
  componentBindings?: ComponentSiteBindings[];
  primarySite?: SiteInfo | null;
  // Group management
  canEdit?: boolean;
  onCreateGroup?: (name: string, color: string, description?: string) => Promise<void>;
  onUpdateGroup?: (groupId: string, name: string, color: string) => Promise<void>;
  onDeleteGroup?: (groupId: string) => Promise<void>;
  activeGroupFilter?: string | null;
  onGroupFilterChange?: (groupId: string | null) => void;
}

/**
 * Find all components that depend on the given component (directly or transitively).
 * Used for STOP action - need to stop dependents first.
 */
export function findDependents(componentId: string, dependencies: Dependency[]): Set<string> {
  const reverseDeps = new Map<string, Set<string>>();
  for (const d of dependencies) {
    if (!reverseDeps.has(d.to_component_id)) {
      reverseDeps.set(d.to_component_id, new Set());
    }
    reverseDeps.get(d.to_component_id)!.add(d.from_component_id);
  }

  const result = new Set<string>();
  const stack = [componentId];

  while (stack.length > 0) {
    const current = stack.pop()!;
    const dependents = reverseDeps.get(current);
    if (dependents) {
      for (const dep of dependents) {
        if (!result.has(dep)) {
          result.add(dep);
          stack.push(dep);
        }
      }
    }
  }

  return result;
}

/**
 * Find all dependencies of the given component (directly or transitively).
 * Used for START_WITH_DEPS action - need to start dependencies first.
 */
export function findDependencies(componentId: string, dependencies: Dependency[]): Set<string> {
  const forwardDeps = new Map<string, Set<string>>();
  for (const d of dependencies) {
    if (!forwardDeps.has(d.from_component_id)) {
      forwardDeps.set(d.from_component_id, new Set());
    }
    forwardDeps.get(d.from_component_id)!.add(d.to_component_id);
  }

  const result = new Set<string>();
  const stack = [componentId];

  while (stack.length > 0) {
    const current = stack.pop()!;
    const deps = forwardDeps.get(current);
    if (deps) {
      for (const dep of deps) {
        if (!result.has(dep)) {
          result.add(dep);
          stack.push(dep);
        }
      }
    }
  }

  return result;
}

/**
 * Compute the full branch for a component (both upstream and downstream).
 */
export function computeBranchHighlight(
  componentId: string,
  dependencies: Dependency[]
): BranchHighlight {
  return {
    selectedId: componentId,
    dependencyIds: findDependencies(componentId, dependencies),
    dependentIds: findDependents(componentId, dependencies),
  };
}

/**
 * Use dagre to compute optimal positions for components.
 * This minimizes edge crossings and prevents overlapping.
 */
function computeDagrePositions(
  components: Component[],
  dependencies: Dependency[]
): Map<string, { x: number; y: number }> {
  const g = new Dagre.graphlib.Graph().setDefaultEdgeLabel(() => ({}));

  g.setGraph({
    rankdir: 'TB',
    nodesep: 100,
    ranksep: 150,
    marginx: 50,
    marginy: 50,
    ranker: 'network-simplex',
  });

  const NODE_WIDTH = 200;
  const NODE_HEIGHT = 80;

  for (const c of components) {
    g.setNode(c.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  }

  for (const d of dependencies) {
    // to (base) is placed above from (dependent) → bases at top, dependents at bottom
    g.setEdge(d.to_component_id, d.from_component_id);
  }

  Dagre.layout(g);

  const positions = new Map<string, { x: number; y: number }>();
  for (const c of components) {
    const node = g.node(c.id);
    if (node) {
      positions.set(c.id, { x: node.x - NODE_WIDTH / 2, y: node.y - NODE_HEIGHT / 2 });
    }
  }

  return positions;
}

function buildNodes(
  components: Component[],
  dependencies: Dependency[],
  groups?: ComponentGroup[],
  onStart?: (id: string) => void,
  onStop?: (id: string) => void,
  onRestart?: (id: string) => void,
  onDiagnose?: (id: string) => void,
  onForceStop?: (id: string) => void,
  onStartWithDeps?: (id: string) => void,
  onRepair?: (id: string) => void,
  onNavigateToApp?: (appId: string) => void,
  editable?: boolean,
  branchHighlight?: BranchHighlight | null,
  impactPreview?: ImpactPreview | null,
  edgeHighlight?: EdgeHighlight | null,
  infraHighlight?: Set<string> | null,
  forceAutoLayout?: boolean,
  allowDrag?: boolean,
  siteBindingsMap?: Map<string, SiteBinding[]>,
  primarySite?: SiteInfo | null,
  activeGroupFilter?: string | null,
): Node[] {
  const groupColorMap: Record<string, string> = {};
  if (groups) {
    for (const g of groups) {
      groupColorMap[g.id] = g.color || '#6366F1';
    }
  }

  const autoPositions = computeDagrePositions(components, dependencies);

  return components.map((c) => {
    // Use saved position only if not forcing auto-layout
    const hasPosition = !forceAutoLayout && c.position_x != null && c.position_y != null;
    const pos = hasPosition
      ? { x: c.position_x!, y: c.position_y! }
      : autoPositions.get(c.id) || { x: 50, y: 50 };

    // Determine highlight state
    let highlightType: 'none' | 'selected' | 'dependency' | 'dependent' | 'impact' | 'edge_endpoint' | 'infra' = 'none';
    let highlightColor: string | undefined;

    // Infrastructure highlight takes precedence when active
    if (infraHighlight && infraHighlight.has(c.id)) {
      highlightType = 'infra';
      highlightColor = '#0EA5E9'; // Sky blue for infrastructure highlight
    } else if (impactPreview) {
      if (c.id === impactPreview.componentId || impactPreview.impactedIds.has(c.id)) {
        highlightType = 'impact';
        highlightColor = impactPreview.action === 'stop' ? '#EF4444' :
                         impactPreview.action === 'start_with_deps' ? '#3B82F6' : '#22C55E';
      }
    } else if (edgeHighlight) {
      // Highlight components connected by the selected edge
      if (c.id === edgeHighlight.sourceId || c.id === edgeHighlight.targetId) {
        highlightType = 'edge_endpoint';
        highlightColor = '#8B5CF6'; // Violet for edge endpoints
      }
    } else if (branchHighlight) {
      if (c.id === branchHighlight.selectedId) {
        highlightType = 'selected';
        highlightColor = '#6366F1'; // Indigo for selected
      } else if (branchHighlight.dependencyIds.has(c.id)) {
        highlightType = 'dependency';
        highlightColor = '#10B981'; // Emerald for dependencies (upstream)
      } else if (branchHighlight.dependentIds.has(c.id)) {
        highlightType = 'dependent';
        highlightColor = '#F59E0B'; // Amber for dependents (downstream)
      }
    }

    return {
      id: c.id,
      type: 'component',
      position: pos,
      draggable: editable || allowDrag,
      data: {
        label: c.name,
        displayName: c.display_name,
        description: c.description,
        icon: c.icon,
        groupColor: c.group_id ? groupColorMap[c.group_id] : undefined,
        state: (c.current_state || 'UNKNOWN') as ComponentState,
        componentType: (c.component_type || 'service') as ComponentType,
        host: c.host,
        // Cluster configuration
        clusterSize: c.cluster_size,
        clusterNodes: c.cluster_nodes,
        // Connectivity status
        connectivityStatus: c.connectivity_status,
        agentHostname: c.agent_hostname,
        agentId: c.agent_id || undefined,
        gatewayId: c.gateway_id || undefined,
        gatewayName: c.gateway_name || undefined,
        // Application reference (for application-type components)
        referencedAppId: c.referenced_app_id || undefined,
        referencedAppName: c.referenced_app_name || undefined,
        // Cross-site probe status
        passiveSiteStatus: c.passive_site_status || undefined,
        // Callbacks
        onStart: editable ? undefined : onStart,
        onStop: editable ? undefined : onStop,
        onRestart: editable ? undefined : onRestart,
        onDiagnose: editable ? undefined : onDiagnose,
        onForceStop: editable ? undefined : onForceStop,
        onStartWithDeps: editable ? undefined : onStartWithDeps,
        onRepair: editable ? undefined : onRepair,
        onNavigateToApp: editable ? undefined : onNavigateToApp,
        editable,
        highlightType,
        highlightColor,
        // Metrics from latest check
        metrics: c.last_check_metrics,
        // Multi-site data
        primarySite: primarySite || undefined,
        siteBindings: siteBindingsMap?.get(c.id)?.map((b) => ({
          site_id: b.site_id,
          site_name: b.site_name,
          site_code: b.site_code,
          site_type: b.site_type,
          is_active: b.is_active,
          agent_hostname: b.agent_hostname,
          has_command_overrides: b.has_command_overrides,
        })),
      },
      // Dim nodes not matching the active group filter
      style: activeGroupFilter && c.group_id !== activeGroupFilter
        ? { opacity: 0.25 }
        : undefined,
    };
  });
}

function buildEdges(
  dependencies: Dependency[],
  editable?: boolean,
  branchHighlight?: BranchHighlight | null,
  impactPreview?: ImpactPreview | null,
  edgeHighlight?: EdgeHighlight | null,
): Edge[] {
  return dependencies.map((d) => {
    let isHighlighted = false;
    let edgeColor = '#94a3b8';
    let animated = false;

    // Check if this specific edge is selected
    if (edgeHighlight && d.id === edgeHighlight.edgeId) {
      isHighlighted = true;
      animated = true;
      edgeColor = '#8B5CF6'; // Violet for selected edge
    }
    // Check if this edge is part of the impact preview
    else if (impactPreview) {
      const allImpacted = new Set(impactPreview.impactedIds);
      allImpacted.add(impactPreview.componentId);
      if (allImpacted.has(d.from_component_id) && allImpacted.has(d.to_component_id)) {
        isHighlighted = true;
        animated = true;
        edgeColor = impactPreview.action === 'stop' ? '#EF4444' :
                    impactPreview.action === 'start_with_deps' ? '#3B82F6' : '#22C55E';
      }
    } else if (branchHighlight) {
      const allInBranch = new Set([
        branchHighlight.selectedId,
        ...branchHighlight.dependencyIds,
        ...branchHighlight.dependentIds,
      ]);

      if (allInBranch.has(d.from_component_id) && allInBranch.has(d.to_component_id)) {
        isHighlighted = true;

        // Determine edge color based on direction
        if (branchHighlight.dependencyIds.has(d.to_component_id)) {
          edgeColor = '#10B981'; // Emerald for upstream path
        } else if (branchHighlight.dependentIds.has(d.from_component_id)) {
          edgeColor = '#F59E0B'; // Amber for downstream path
        } else {
          edgeColor = '#6366F1'; // Indigo
        }
      }
    }

    return {
      id: d.id,
      source: d.from_component_id,
      target: d.to_component_id,
      type: 'default', // Use bezier curves for smoother lines
      animated,
      selectable: true,
      deletable: editable,
      style: {
        stroke: edgeColor,
        strokeWidth: isHighlighted ? 3 : 2,
      },
      label: d.dep_type !== 'strong' ? d.dep_type : undefined,
    };
  });
}

function AppMapInner({
  components,
  dependencies,
  groups,
  onSelectComponent,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  selectedComponentId,
  onStartAll,
  onStopAll,
  onRestartErrorBranch,
  onShare,
  onToggleActivity,
  activityOpen,
  onSwitchover,
  canManage,
  onStartComponent,
  onStopComponent,
  onRestartComponent,
  onDiagnoseComponent,
  onForceStopComponent,
  onStartWithDepsComponent,
  onRepairComponent,
  onNavigateToApp,
  canOperate,
  editable,
  onNodePositionChange,
  onConnect,
  onDeleteEdge,
  onDeleteNode,
  onNodeDoubleClick,
  onDrop,
  impactPreview,
  branchHighlight,
  edgeHighlight,
  onEdgeClick,
  allowDrag = true, // Default to allowing drag in view mode
  onSaveLayout,
  isSavingLayout,
  componentBindings,
  primarySite,
  canEdit,
  onCreateGroup,
  onUpdateGroup,
  onDeleteGroup,
  activeGroupFilter,
  onGroupFilterChange,
}: AppMapProps) {
  const reactFlowWrapper = useRef<HTMLDivElement>(null);
  const { screenToFlowPosition, fitView } = useReactFlow();

  // Context menu state
  const [contextMenu, setContextMenu] = useState<MapContextMenuProps | null>(null);

  // "Connect to..." mode: when set, next node click completes the connection
  const [connectFromNodeId, setConnectFromNodeId] = useState<string | null>(null);

  // Infrastructure highlight (when hovering over agents/gateways in summary)
  const [infraHighlight, setInfraHighlight] = useState<Set<string> | null>(null);

  // Force auto-layout flag (ignores saved positions when true)
  // Start with true so auto-layout is applied on initial load
  const [forceAutoLayout, setForceAutoLayout] = useState(true);

  // Track pending position changes (for save layout feature)
  const [pendingPositions, setPendingPositions] = useState<Map<string, { x: number; y: number }>>(new Map());

  const handleHighlightComponents = useCallback((componentIds: string[]) => {
    setInfraHighlight(new Set(componentIds));
  }, []);

  const handleClearHighlight = useCallback(() => {
    setInfraHighlight(null);
  }, []);

  // Capture all node positions for saving after auto-layout
  const captureAutoLayoutPositions = useCallback(() => {
    const autoPositions = computeDagrePositions(components, dependencies);
    const newPending = new Map<string, { x: number; y: number }>();
    for (const [id, pos] of autoPositions) {
      newPending.set(id, pos);
    }
    setPendingPositions(newPending);
  }, [components, dependencies]);

  const handleAutoLayout = useCallback(() => {
    setForceAutoLayout(true);
    // Capture positions so they can be saved
    captureAutoLayoutPositions();
    // Fit view after layout is applied
    requestAnimationFrame(() => {
      fitView({ padding: 0.2 });
    });
  }, [fitView, captureAutoLayoutPositions]);

  const handleSaveLayout = useCallback(() => {
    if (pendingPositions.size === 0 || !onSaveLayout) return;
    const positions = Array.from(pendingPositions.entries()).map(([id, pos]) => ({
      id,
      x: pos.x,
      y: pos.y,
    }));
    onSaveLayout(positions);
    setPendingPositions(new Map());
  }, [pendingPositions, onSaveLayout]);

  // Build site bindings lookup map
  const siteBindingsMap = useMemo(
    () => componentBindings ? groupBindingsByComponent(componentBindings) : undefined,
    [componentBindings],
  );

  const initialNodes = useMemo(
    () => buildNodes(
      components,
      dependencies,
      groups,
      onStartComponent,
      onStopComponent,
      onRestartComponent,
      onDiagnoseComponent,
      onForceStopComponent,
      onStartWithDepsComponent,
      onRepairComponent,
      onNavigateToApp,
      editable,
      branchHighlight,
      impactPreview,
      edgeHighlight,
      infraHighlight,
      forceAutoLayout,
      allowDrag,
      siteBindingsMap,
      primarySite,
      activeGroupFilter,
    ),
    [components, dependencies, groups, onStartComponent, onStopComponent, onRestartComponent, onDiagnoseComponent, onForceStopComponent, onStartWithDepsComponent, onRepairComponent, onNavigateToApp, editable, branchHighlight, impactPreview, edgeHighlight, infraHighlight, forceAutoLayout, allowDrag, siteBindingsMap, primarySite, activeGroupFilter],
  );

  const initialEdges = useMemo(
    () => buildEdges(dependencies, editable, branchHighlight, impactPreview, edgeHighlight),
    [dependencies, editable, branchHighlight, impactPreview, edgeHighlight]
  );

  const [nodes, setNodes] = useNodesState(initialNodes);
  const [edges, setEdges] = useEdgesState(initialEdges);

  useEffect(() => {
    setNodes(initialNodes);
  }, [initialNodes, setNodes]);

  useEffect(() => {
    // Preserve React Flow's selection state when syncing edges from props,
    // otherwise selecting an edge and pressing Delete would not work because
    // the parent re-renders edges (e.g. edgeHighlight change) and the
    // selected flag is lost.
    setEdges((prev) => {
      const selectedIds = new Set(prev.filter((e) => e.selected).map((e) => e.id));
      if (selectedIds.size === 0) return initialEdges;
      return initialEdges.map((e) =>
        selectedIds.has(e.id) ? { ...e, selected: true } : e,
      );
    });
  }, [initialEdges, setEdges]);

  // Fit view when components change (e.g., switching apps in supervision mode)
  useEffect(() => {
    if (components.length > 0) {
      // Small delay to ensure nodes are rendered before fitting
      const timer = setTimeout(() => {
        fitView({ padding: 0.2 });
      }, 100);
      return () => clearTimeout(timer);
    }
  }, [components, fitView]);

  const handleNodesChange = useCallback(
    (changes: NodeChange[]) => {
      setNodes((nds) => applyNodeChanges(changes, nds));

      // Track position changes for edit mode callback
      if (editable && onNodePositionChange) {
        changes.forEach((change) => {
          if (change.type === 'position' && change.position && change.dragging === false) {
            onNodePositionChange(change.id, change.position.x, change.position.y);
          }
        });
      }

      // Track position changes for save layout feature (view mode)
      if (!editable && allowDrag && onSaveLayout) {
        changes.forEach((change) => {
          if (change.type === 'position' && change.position && change.dragging === false) {
            setPendingPositions((prev) => {
              const next = new Map(prev);
              next.set(change.id, { x: change.position!.x, y: change.position!.y });
              return next;
            });
          }
        });
      }

      if (editable && onDeleteNode) {
        changes.forEach((change) => {
          if (change.type === 'remove') {
            onDeleteNode(change.id);
          }
        });
      }
    },
    [setNodes, editable, onNodePositionChange, onDeleteNode, allowDrag, onSaveLayout],
  );

  const handleEdgesChange = useCallback(
    (changes: EdgeChange[]) => {
      // Intercept remove changes: delegate to confirmation dialog instead of
      // applying immediately (which would hide the edge before the user confirms).
      const removeIds: string[] = [];
      const others: EdgeChange[] = [];
      for (const change of changes) {
        if (change.type === 'remove') {
          removeIds.push(change.id);
        } else {
          others.push(change);
        }
      }

      // Apply non-removal changes normally (select, etc.)
      if (others.length > 0) {
        setEdges((eds) => applyEdgeChanges(others, eds));
      }

      // Trigger deletion dialog for each removal (edge stays visible until confirmed)
      if (editable && onDeleteEdge) {
        removeIds.forEach((id) => {
          onDeleteEdge(id);
        });
      }
    },
    [setEdges, editable, onDeleteEdge],
  );

  // Helper to get component name by id
  const getComponentName = useCallback(
    (id: string) => components.find((c) => c.id === id)?.name || id.slice(0, 8),
    [components],
  );

  const handleConnect = useCallback(
    (connection: Connection) => {
      if (!editable || !onConnect) return;
      if (connection.source && connection.target) {
        if (connection.source === connection.target) {
          toast.error('Cannot connect a component to itself');
          return;
        }
        // Check for duplicate
        const exists = dependencies.some(
          (d) => d.from_component_id === connection.source && d.to_component_id === connection.target,
        );
        if (exists) {
          toast.warning('This dependency already exists');
          return;
        }
        onConnect(connection.source, connection.target);
        toast.success(`Dependency created: ${getComponentName(connection.source)} → ${getComponentName(connection.target)}`);
      }
    },
    [editable, onConnect, dependencies, getComponentName],
  );

  const onNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      // If in "Connect to..." mode, complete the connection
      if (connectFromNodeId && editable && onConnect) {
        if (connectFromNodeId !== node.id) {
          const exists = dependencies.some(
            (d) => d.from_component_id === connectFromNodeId && d.to_component_id === node.id,
          );
          if (exists) {
            toast.warning('This dependency already exists');
          } else {
            onConnect(connectFromNodeId, node.id);
            toast.success(`Dependency created: ${getComponentName(connectFromNodeId)} → ${getComponentName(node.id)}`);
          }
        }
        setConnectFromNodeId(null);
        return;
      }
      onSelectComponent(node.id);
    },
    [onSelectComponent, connectFromNodeId, editable, onConnect, dependencies, getComponentName],
  );

  const handleEdgeClick = useCallback(
    (_: React.MouseEvent, edge: Edge) => {
      if (onEdgeClick) {
        onEdgeClick(edge.id, edge.source, edge.target);
      }
    },
    [onEdgeClick],
  );

  const handleNodeDoubleClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      if (editable && onNodeDoubleClick) {
        onNodeDoubleClick(node.id);
      }
    },
    [editable, onNodeDoubleClick],
  );

  const onPaneClick = useCallback(() => {
    onSelectComponent(null);
    setContextMenu(null);
    setConnectFromNodeId(null);
  }, [onSelectComponent]);

  const onDragOver = useCallback((event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
  }, []);

  const handleDrop = useCallback(
    (event: DragEvent<HTMLDivElement>) => {
      event.preventDefault();
      if (!editable || !onDrop) return;

      const type = event.dataTransfer.getData('application/reactflow');
      if (!type) return;

      const position = screenToFlowPosition({
        x: event.clientX,
        y: event.clientY,
      });

      onDrop(type, position);
    },
    [editable, onDrop, screenToFlowPosition],
  );

  // Right-click context menu for edges
  const handleEdgeContextMenu = useCallback(
    (event: React.MouseEvent, edge: Edge) => {
      if (!editable) return;
      event.preventDefault();
      setContextMenu({
        type: 'edge',
        position: { x: event.clientX, y: event.clientY },
        edgeId: edge.id,
        sourceName: getComponentName(edge.source),
        targetName: getComponentName(edge.target),
        onDelete: (id: string) => {
          if (onDeleteEdge) onDeleteEdge(id);
        },
        onClose: () => setContextMenu(null),
      });
    },
    [editable, getComponentName, onDeleteEdge],
  );

  // Right-click context menu for nodes
  const handleNodeContextMenu = useCallback(
    (event: React.MouseEvent, node: Node) => {
      if (!editable) return;
      event.preventDefault();
      setContextMenu({
        type: 'node',
        position: { x: event.clientX, y: event.clientY },
        nodeId: node.id,
        nodeName: getComponentName(node.id),
        onDelete: (id: string) => {
          if (onDeleteNode) onDeleteNode(id);
        },
        onEdit: (id: string) => {
          if (onNodeDoubleClick) onNodeDoubleClick(id);
        },
        onStartConnect: (id: string) => {
          setConnectFromNodeId(id);
          toast.info(`Click another component to connect from "${getComponentName(id)}"`);
        },
        onClose: () => setContextMenu(null),
      });
    },
    [editable, getComponentName, onDeleteNode, onNodeDoubleClick],
  );

  // Determine highlight color for minimap
  const getNodeColor = useCallback((node: Node): string => {
    const state = (node.data?.state as ComponentState) || 'UNKNOWN';
    const highlightColor = node.data?.highlightColor as string | undefined;
    if (highlightColor) {
      return highlightColor;
    }
    return STATE_COLORS[state]?.border || '#BDBDBD';
  }, []);

  return (
    <div className="w-full h-full relative" ref={reactFlowWrapper}>
      {/* "Connect to..." mode indicator banner */}
      {connectFromNodeId && (
        <div className="absolute top-2 left-1/2 -translate-x-1/2 z-50 bg-indigo-600 text-white px-4 py-2 rounded-lg shadow-lg text-sm flex items-center gap-2 animate-in fade-in-0 slide-in-from-top-2">
          <span>Click a component to connect from <strong>{getComponentName(connectFromNodeId)}</strong></span>
          <button
            className="ml-2 text-xs bg-white/20 hover:bg-white/30 rounded px-2 py-0.5 transition-colors"
            onClick={() => { setConnectFromNodeId(null); toast.dismiss(); }}
          >
            Cancel
          </button>
        </div>
      )}
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={handleNodesChange}
        onEdgesChange={handleEdgesChange}
        onConnect={handleConnect}
        onNodeClick={onNodeClick}
        onEdgeClick={handleEdgeClick}
        onNodeDoubleClick={handleNodeDoubleClick}
        onPaneClick={onPaneClick}
        onDragOver={onDragOver}
        onDrop={handleDrop}
        onEdgeContextMenu={handleEdgeContextMenu}
        onNodeContextMenu={handleNodeContextMenu}
        nodeTypes={nodeTypes}
        fitView
        fitViewOptions={{ padding: 0.2 }}
        minZoom={0.1}
        maxZoom={2}
        nodesDraggable={editable || allowDrag}
        nodesConnectable={editable}
        elementsSelectable={true}
        deleteKeyCode={editable ? ['Delete', 'Backspace'] : null}
        connectionLineStyle={{ stroke: '#6366F1', strokeWidth: 2 }}
        defaultEdgeOptions={{
          type: 'default', // Bezier curves
          style: { stroke: '#94a3b8', strokeWidth: 2 },
        }}
      >
        <Background variant={BackgroundVariant.Dots} gap={20} size={1} />
        <Controls showInteractive={false} />
        <MiniMap
          nodeColor={getNodeColor}
          maskColor="rgba(0,0,0,0.1)"
          className="!bg-card !border-border"
        />
        {!editable ? (
          <>
            <MapToolbar
              onStartAll={onStartAll}
              onStopAll={onStopAll}
              onRestartErrorBranch={onRestartErrorBranch}
              onSwitchover={onSwitchover}
              onShare={onShare}
              onToggleActivity={onToggleActivity}
              activityOpen={activityOpen}
              canOperate={canOperate}
              canManage={canManage}
              canEdit={canEdit}
              onAutoLayout={handleAutoLayout}
              onSaveLayout={onSaveLayout ? handleSaveLayout : undefined}
              hasUnsavedPositions={pendingPositions.size > 0}
              isSavingLayout={isSavingLayout}
              groups={groups}
              components={components}
              onCreateGroup={onCreateGroup}
              onUpdateGroup={onUpdateGroup}
              onDeleteGroup={onDeleteGroup}
              activeGroupFilter={activeGroupFilter}
              onGroupFilterChange={onGroupFilterChange}
            />
            <InfrastructureSummary
              components={components}
              onHighlightComponents={handleHighlightComponents}
              onClearHighlight={handleClearHighlight}
            />
          </>
        ) : (
          /* Edit-mode toolbar: group management + layout */
          <MapToolbar
            canEdit
            onAutoLayout={handleAutoLayout}
            groups={groups}
            components={components}
            onCreateGroup={onCreateGroup}
            onUpdateGroup={onUpdateGroup}
            onDeleteGroup={onDeleteGroup}
            activeGroupFilter={activeGroupFilter}
            onGroupFilterChange={onGroupFilterChange}
          />
        )}
        {/* Edge toolbar: visible delete button when an edge is selected in edit mode */}
        {editable && edgeHighlight && onDeleteEdge && (
          <EdgeToolbar
            edgeId={edgeHighlight.edgeId}
            sourceId={edgeHighlight.sourceId}
            targetId={edgeHighlight.targetId}
            sourceName={getComponentName(edgeHighlight.sourceId)}
            targetName={getComponentName(edgeHighlight.targetId)}
            onDelete={onDeleteEdge}
          />
        )}
      </ReactFlow>
      {/* Context menu (rendered outside ReactFlow to use fixed positioning) */}
      {contextMenu && <MapContextMenu {...contextMenu} />}
    </div>
  );
}

export function AppMap(props: AppMapProps) {
  return (
    <ReactFlowProvider>
      <AppMapInner {...props} />
    </ReactFlowProvider>
  );
}
