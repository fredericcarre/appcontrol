import { useCallback, useMemo, useRef, DragEvent } from 'react';
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
  addEdge,
  NodeChange,
  EdgeChange,
  applyNodeChanges,
  applyEdgeChanges,
  useReactFlow,
  ReactFlowProvider,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { ComponentNode } from './ComponentNode';
import { MapToolbar } from './MapToolbar';
import { Component, Dependency, ComponentGroup } from '@/api/apps';
import { ComponentState, ComponentType, STATE_COLORS } from '@/lib/colors';

const nodeTypes: NodeTypes = {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  component: ComponentNode as any,
};

interface AppMapProps {
  components: Component[];
  dependencies: Dependency[];
  groups?: ComponentGroup[];
  onSelectComponent: (id: string | null) => void;
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
  canOperate?: boolean;
  // Edit mode props
  editable?: boolean;
  onNodePositionChange?: (nodeId: string, x: number, y: number) => void;
  onConnect?: (sourceId: string, targetId: string) => void;
  onDeleteEdge?: (edgeId: string) => void;
  onDeleteNode?: (nodeId: string) => void;
  onNodeDoubleClick?: (nodeId: string) => void;
  onDrop?: (type: string, position: { x: number; y: number }) => void;
}

function buildNodes(
  components: Component[],
  groups?: ComponentGroup[],
  onStart?: (id: string) => void,
  onStop?: (id: string) => void,
  onRestart?: (id: string) => void,
  onDiagnose?: (id: string) => void,
  onForceStop?: (id: string) => void,
  onStartWithDeps?: (id: string) => void,
  editable?: boolean,
): Node[] {
  const groupColorMap: Record<string, string> = {};
  if (groups) {
    for (const g of groups) {
      groupColorMap[g.id] = g.color || '#6366F1';
    }
  }

  return components.map((c, i) => ({
    id: c.id,
    type: 'component',
    position: {
      x: c.position_x ?? (i % 4) * 250 + 50,
      y: c.position_y ?? Math.floor(i / 4) * 180 + 50,
    },
    draggable: editable,
    data: {
      label: c.name,
      displayName: c.display_name,
      description: c.description,
      icon: c.icon,
      groupColor: c.group_id ? groupColorMap[c.group_id] : undefined,
      state: (c.state || 'UNKNOWN') as ComponentState,
      componentType: (c.component_type || 'service') as ComponentType,
      host: c.host,
      onStart: editable ? undefined : onStart,
      onStop: editable ? undefined : onStop,
      onRestart: editable ? undefined : onRestart,
      onDiagnose: editable ? undefined : onDiagnose,
      onForceStop: editable ? undefined : onForceStop,
      onStartWithDeps: editable ? undefined : onStartWithDeps,
      editable,
    },
  }));
}

function buildEdges(dependencies: Dependency[], editable?: boolean): Edge[] {
  return dependencies.map((d) => ({
    id: d.id,
    source: d.from_component_id,
    target: d.to_component_id,
    type: 'smoothstep',
    animated: false,
    deletable: editable,
    style: { stroke: '#94a3b8', strokeWidth: 2 },
    label: d.dep_type !== 'strong' ? d.dep_type : undefined,
  }));
}

function AppMapInner({
  components,
  dependencies,
  groups,
  onSelectComponent,
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
  canOperate,
  editable,
  onNodePositionChange,
  onConnect,
  onDeleteEdge,
  onDeleteNode,
  onNodeDoubleClick,
  onDrop,
}: AppMapProps) {
  const reactFlowWrapper = useRef<HTMLDivElement>(null);
  const { screenToFlowPosition } = useReactFlow();

  const initialNodes = useMemo(
    () => buildNodes(
      components,
      groups,
      onStartComponent,
      onStopComponent,
      onRestartComponent,
      onDiagnoseComponent,
      onForceStopComponent,
      onStartWithDepsComponent,
      editable,
    ),
    [components, groups, onStartComponent, onStopComponent, onRestartComponent, onDiagnoseComponent, onForceStopComponent, onStartWithDepsComponent, editable],
  );
  const initialEdges = useMemo(() => buildEdges(dependencies, editable), [dependencies, editable]);

  const [nodes, setNodes, onNodesChangeInternal] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChangeInternal] = useEdgesState(initialEdges);

  const handleNodesChange = useCallback(
    (changes: NodeChange[]) => {
      // Apply changes to local state
      setNodes((nds) => applyNodeChanges(changes, nds));

      // Track position changes for saving
      if (editable && onNodePositionChange) {
        changes.forEach((change) => {
          if (change.type === 'position' && change.position && change.dragging === false) {
            // Position change completed (drag ended)
            onNodePositionChange(change.id, change.position.x, change.position.y);
          }
        });
      }

      // Handle node removal
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
      // Handle edge removal
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

  // Drag and drop handlers
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

  return (
    <div className="w-full h-full relative" ref={reactFlowWrapper}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={handleNodesChange}
        onEdgesChange={handleEdgesChange}
        onConnect={handleConnect}
        onNodeClick={onNodeClick}
        onNodeDoubleClick={handleNodeDoubleClick}
        onPaneClick={onPaneClick}
        onDragOver={onDragOver}
        onDrop={handleDrop}
        nodeTypes={nodeTypes}
        fitView
        fitViewOptions={{ padding: 0.2 }}
        minZoom={0.1}
        maxZoom={2}
        nodesDraggable={editable}
        nodesConnectable={editable}
        elementsSelectable={true}
        deleteKeyCode={editable ? 'Delete' : null}
        connectionLineStyle={{ stroke: '#6366F1', strokeWidth: 2 }}
        defaultEdgeOptions={{
          type: 'smoothstep',
          style: { stroke: '#94a3b8', strokeWidth: 2 },
        }}
      >
        <Background variant={BackgroundVariant.Dots} gap={20} size={1} />
        <Controls showInteractive={false} />
        <MiniMap
          nodeColor={(node) => {
            const state = (node.data?.state as ComponentState) || 'UNKNOWN';
            return STATE_COLORS[state]?.border || '#BDBDBD';
          }}
          maskColor="rgba(0,0,0,0.1)"
          className="!bg-card !border-border"
        />
        {!editable && (
          <MapToolbar
            onStartAll={onStartAll}
            onStopAll={onStopAll}
            onRestartErrorBranch={onRestartErrorBranch}
            onShare={onShare}
            onToggleActivity={onToggleActivity}
            activityOpen={activityOpen}
            canOperate={canOperate}
          />
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
