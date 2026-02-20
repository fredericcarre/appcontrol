import { useCallback, useMemo, useState } from 'react';
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
import { Component, Dependency } from '@/api/apps';
import { ComponentState, ComponentType, STATE_COLORS } from '@/lib/colors';

const nodeTypes: NodeTypes = {
  component: ComponentNode as any,
};

interface AppMapProps {
  components: Component[];
  dependencies: Dependency[];
  onSelectComponent: (id: string | null) => void;
  onStartAll?: () => void;
  onStopAll?: () => void;
  onRestartErrorBranch?: () => void;
  onShare?: () => void;
  onStartComponent?: (id: string) => void;
  onStopComponent?: (id: string) => void;
  onRestartComponent?: (id: string) => void;
  onDiagnoseComponent?: (id: string) => void;
  canOperate?: boolean;
}

function buildNodes(
  components: Component[],
  onStart?: (id: string) => void,
  onStop?: (id: string) => void,
  onRestart?: (id: string) => void,
  onDiagnose?: (id: string) => void,
): Node[] {
  return components.map((c, i) => ({
    id: c.id,
    type: 'component',
    position: {
      x: c.position_x ?? (i % 4) * 250 + 50,
      y: c.position_y ?? Math.floor(i / 4) * 180 + 50,
    },
    data: {
      label: c.name,
      state: (c.state || 'UNKNOWN') as ComponentState,
      componentType: (c.component_type || 'service') as ComponentType,
      host: c.host,
      onStart,
      onStop,
      onRestart,
      onDiagnose,
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
  onSelectComponent,
  onStartAll,
  onStopAll,
  onRestartErrorBranch,
  onShare,
  onStartComponent,
  onStopComponent,
  onRestartComponent,
  onDiagnoseComponent,
  canOperate,
}: AppMapProps) {
  const initialNodes = useMemo(
    () => buildNodes(components, onStartComponent, onStopComponent, onRestartComponent, onDiagnoseComponent),
    [components, onStartComponent, onStopComponent, onRestartComponent, onDiagnoseComponent],
  );
  const initialEdges = useMemo(() => buildEdges(dependencies), [dependencies]);

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initialEdges);

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
