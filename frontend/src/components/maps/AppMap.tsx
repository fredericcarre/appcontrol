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
import { ComponentNode } from './ComponentNode';
import { MapToolbar } from './MapToolbar';
import { InfrastructureSummary } from './InfrastructureSummary';
import { Component, Dependency, ComponentGroup } from '@/api/apps';
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
  onStartComponent?: (id: string) => void;
  onStopComponent?: (id: string) => void;
  onRestartComponent?: (id: string) => void;
  onDiagnoseComponent?: (id: string) => void;
  onForceStopComponent?: (id: string) => void;
  onStartWithDepsComponent?: (id: string) => void;
  onRepairComponent?: (id: string) => void;
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
    // from depends on to → from is placed above to (dependents at top, bases at bottom)
    g.setEdge(d.from_component_id, d.to_component_id);
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
  editable?: boolean,
  branchHighlight?: BranchHighlight | null,
  impactPreview?: ImpactPreview | null,
  edgeHighlight?: EdgeHighlight | null,
  infraHighlight?: Set<string> | null,
  forceAutoLayout?: boolean,
  allowDrag?: boolean,
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
        // Connectivity status
        connectivityStatus: c.connectivity_status,
        agentHostname: c.agent_hostname,
        agentId: c.agent_id || undefined,
        gatewayId: c.gateway_id || undefined,
        // Callbacks
        onStart: editable ? undefined : onStart,
        onStop: editable ? undefined : onStop,
        onRestart: editable ? undefined : onRestart,
        onDiagnose: editable ? undefined : onDiagnose,
        onForceStop: editable ? undefined : onForceStop,
        onStartWithDeps: editable ? undefined : onStartWithDeps,
        onRepair: editable ? undefined : onRepair,
        editable,
        highlightType,
        highlightColor,
      },
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
  onStartComponent,
  onStopComponent,
  onRestartComponent,
  onDiagnoseComponent,
  onForceStopComponent,
  onStartWithDepsComponent,
  onRepairComponent,
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
}: AppMapProps) {
  const reactFlowWrapper = useRef<HTMLDivElement>(null);
  const { screenToFlowPosition, fitView } = useReactFlow();

  // Infrastructure highlight (when hovering over agents/gateways in summary)
  const [infraHighlight, setInfraHighlight] = useState<Set<string> | null>(null);

  // Force auto-layout flag (ignores saved positions when true)
  const [forceAutoLayout, setForceAutoLayout] = useState(false);

  const handleHighlightComponents = useCallback((componentIds: string[]) => {
    setInfraHighlight(new Set(componentIds));
  }, []);

  const handleClearHighlight = useCallback(() => {
    setInfraHighlight(null);
  }, []);

  const handleAutoLayout = useCallback(() => {
    setForceAutoLayout(true);
    // Fit view after layout is applied
    requestAnimationFrame(() => {
      fitView({ padding: 0.2 });
    });
  }, [fitView]);

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
      editable,
      branchHighlight,
      impactPreview,
      edgeHighlight,
      infraHighlight,
      forceAutoLayout,
      allowDrag,
    ),
    [components, dependencies, groups, onStartComponent, onStopComponent, onRestartComponent, onDiagnoseComponent, onForceStopComponent, onStartWithDepsComponent, onRepairComponent, editable, branchHighlight, impactPreview, edgeHighlight, infraHighlight, forceAutoLayout, allowDrag],
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
    setEdges(initialEdges);
  }, [initialEdges, setEdges]);

  const handleNodesChange = useCallback(
    (changes: NodeChange[]) => {
      setNodes((nds) => applyNodeChanges(changes, nds));

      if (editable && onNodePositionChange) {
        changes.forEach((change) => {
          if (change.type === 'position' && change.position && change.dragging === false) {
            onNodePositionChange(change.id, change.position.x, change.position.y);
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
    [setNodes, editable, onNodePositionChange, onDeleteNode],
  );

  const handleEdgesChange = useCallback(
    (changes: EdgeChange[]) => {
      if (editable && onDeleteEdge) {
        changes.forEach((change) => {
          if (change.type === 'remove') {
            onDeleteEdge(change.id);
          }
        });
      }
      setEdges((eds) => applyEdgeChanges(changes, eds));
    },
    [setEdges, editable, onDeleteEdge],
  );

  const handleConnect = useCallback(
    (connection: Connection) => {
      if (!editable || !onConnect) return;
      if (connection.source && connection.target) {
        onConnect(connection.source, connection.target);
      }
    },
    [editable, onConnect],
  );

  const onNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      onSelectComponent(node.id);
    },
    [onSelectComponent],
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
        {!editable && (
          <>
            <MapToolbar
              onStartAll={onStartAll}
              onStopAll={onStopAll}
              onRestartErrorBranch={onRestartErrorBranch}
              onShare={onShare}
              onToggleActivity={onToggleActivity}
              activityOpen={activityOpen}
              canOperate={canOperate}
              onAutoLayout={handleAutoLayout}
            />
            <InfrastructureSummary
              components={components}
              onHighlightComponents={handleHighlightComponents}
              onClearHighlight={handleClearHighlight}
            />
          </>
        )}
      </ReactFlow>
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
