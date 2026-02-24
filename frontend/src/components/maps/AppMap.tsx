import { useCallback, useMemo } from 'react';
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
  onStartComponent?: (id: string) => void;
  onStopComponent?: (id: string) => void;
  onRestartComponent?: (id: string) => void;
  onDiagnoseComponent?: (id: string) => void;
  onForceStopComponent?: (id: string) => void;
  onStartWithDepsComponent?: (id: string) => void;
  canOperate?: boolean;
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
    data: {
      label: c.name,
      displayName: c.display_name,
      description: c.description,
      icon: c.icon,
      groupColor: c.group_id ? groupColorMap[c.group_id] : undefined,
      state: (c.state || 'UNKNOWN') as ComponentState,
      componentType: (c.component_type || 'service') as ComponentType,
      host: c.host,
      onStart,
      onStop,
      onRestart,
      onDiagnose,
      onForceStop,
      onStartWithDeps,
    },
  }));
}

function buildEdges(dependencies: Dependency[]): Edge[] {
  return dependencies.map((d) => ({
    id: d.id,
    source: d.from_component_id,
    target: d.to_component_id,
    type: 'smoothstep',
    animated: false,
    style: { stroke: '#94a3b8', strokeWidth: 2 },
    label: d.dep_type !== 'strong' ? d.dep_type : undefined,
  }));
}

export function AppMap({
  components,
  dependencies,
  groups,
  onSelectComponent,
  onStartAll,
  onStopAll,
  onRestartErrorBranch,
  onShare,
  onStartComponent,
  onStopComponent,
  onRestartComponent,
  onDiagnoseComponent,
  onForceStopComponent,
  onStartWithDepsComponent,
  canOperate,
}: AppMapProps) {
  const initialNodes = useMemo(
    () => buildNodes(components, groups, onStartComponent, onStopComponent, onRestartComponent, onDiagnoseComponent, onForceStopComponent, onStartWithDepsComponent),
    [components, groups, onStartComponent, onStopComponent, onRestartComponent, onDiagnoseComponent, onForceStopComponent, onStartWithDepsComponent],
  );
  const initialEdges = useMemo(() => buildEdges(dependencies), [dependencies]);

  const [nodes, , onNodesChange] = useNodesState(initialNodes);
  const [edges, , onEdgesChange] = useEdgesState(initialEdges);

  const onNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      onSelectComponent(node.id);
    },
    [onSelectComponent],
  );

  const onPaneClick = useCallback(() => {
    onSelectComponent(null);
  }, [onSelectComponent]);

  return (
    <div className="w-full h-full relative">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={onNodeClick}
        onPaneClick={onPaneClick}
        nodeTypes={nodeTypes}
        fitView
        fitViewOptions={{ padding: 0.2 }}
        minZoom={0.1}
        maxZoom={2}
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
        <MapToolbar
          onStartAll={onStartAll}
          onStopAll={onStopAll}
          onRestartErrorBranch={onRestartErrorBranch}
          onShare={onShare}
          canOperate={canOperate}
        />
      </ReactFlow>
    </div>
  );
}
