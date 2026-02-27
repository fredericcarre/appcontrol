import { useCallback, useMemo, useEffect, useRef } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  useReactFlow,
  ReactFlowProvider,
  BackgroundVariant,
  type NodeTypes,
  type EdgeTypes,
  type Node,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { Loader2 } from 'lucide-react';
import { useTopologyLayout } from './TopologyMap.hooks';
import { HostGroupNode } from './HostGroupNode';
import { ServiceNode } from './ServiceNode';
import { ExternalNode } from './ExternalNode';
import { BatchJobNode } from './BatchJobNode';
import { DependencyEdge } from './DependencyEdge';
import { UnresolvedEdge } from './UnresolvedEdge';
import { COMPONENT_TYPE_ICONS, type ComponentType } from '@/lib/colors';
import { useDiscoveryStore } from '@/stores/discovery';

const nodeTypes: NodeTypes = {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  hostGroup: HostGroupNode as any,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  service: ServiceNode as any,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  external: ExternalNode as any,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  batch: BatchJobNode as any,
};

const edgeTypes: EdgeTypes = {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  dependency: DependencyEdge as any,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  unresolved: UnresolvedEdge as any,
};

function TopologyMapInner() {
  const {
    correlationResult,
    enabledServiceIndices,
    showUnresolved,
    showBatchJobs,
    getEffectiveName,
    getEffectiveType,
    highlightedServiceIndex,
    toggleServiceEnabled,
    setSelectedServiceIndex,
  } = useDiscoveryStore();

  const onToggle = useCallback((idx: number) => toggleServiceEnabled(idx), [toggleServiceEnabled]);
  const onSelect = useCallback((idx: number) => setSelectedServiceIndex(idx), [setSelectedServiceIndex]);

  const { nodes: layoutNodes, edges: layoutEdges, isLayouting } = useTopologyLayout({
    correlationResult,
    enabledIndices: enabledServiceIndices,
    showUnresolved,
    showBatchJobs,
    getEffectiveName,
    getEffectiveType,
    highlightedServiceIndex,
    onToggle,
    onSelect,
  });

  const [nodes, setNodes, onNodesChange] = useNodesState(layoutNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(layoutEdges);
  const { fitView } = useReactFlow();
  const prevLayoutRef = useRef<string>('');

  // Sync layout changes to state
  useEffect(() => {
    const key = layoutNodes.map((n) => n.id).join(',');
    if (key !== prevLayoutRef.current) {
      setNodes(layoutNodes);
      setEdges(layoutEdges);
      prevLayoutRef.current = key;
      // Fit view after a short delay for render
      setTimeout(() => fitView({ padding: 0.15, duration: 300 }), 50);
    } else {
      // Update data without repositioning (for highlight/enable toggles)
      setNodes((prev) =>
        prev.map((n) => {
          const updated = layoutNodes.find((ln) => ln.id === n.id);
          return updated ? { ...n, data: updated.data } : n;
        })
      );
      setEdges(layoutEdges);
    }
  }, [layoutNodes, layoutEdges, setNodes, setEdges, fitView]);

  const onNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      if (node.type === 'service') {
        const idx = (node.data as { serviceIndex: number }).serviceIndex;
        setSelectedServiceIndex(idx);
      }
    },
    [setSelectedServiceIndex]
  );

  const onPaneClick = useCallback(() => {
    setSelectedServiceIndex(null);
  }, [setSelectedServiceIndex]);

  const miniMapNodeColor = useMemo(
    () => (node: Node) => {
      if (node.type === 'hostGroup') return '#e2e8f0';
      if (node.type === 'external') return '#94a3b8';
      if (node.type === 'batch') return '#F59E0B';
      const ct = (node.data as { componentType?: string })?.componentType || 'service';
      return COMPONENT_TYPE_ICONS[ct as ComponentType]?.color || '#37474F';
    },
    []
  );

  return (
    <div className="w-full h-full relative">
      {isLayouting && (
        <div className="absolute inset-0 z-20 flex items-center justify-center bg-background/60 backdrop-blur-sm">
          <div className="flex items-center gap-3 bg-card p-4 rounded-lg shadow-lg border">
            <Loader2 className="h-5 w-5 animate-spin text-primary" />
            <span className="text-sm font-medium">Computing topology layout...</span>
          </div>
        </div>
      )}
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={onNodeClick}
        onPaneClick={onPaneClick}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        fitView
        fitViewOptions={{ padding: 0.15 }}
        minZoom={0.1}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
      >
        <Background variant={BackgroundVariant.Dots} gap={20} size={1} color="#e2e8f0" />
        <Controls showInteractive={false} className="!bg-card !border-border !shadow-md" />
        <MiniMap
          nodeColor={miniMapNodeColor}
          maskColor="rgba(0,0,0,0.08)"
          className="!bg-card !border-border !shadow-md !rounded-lg"
          pannable
          zoomable
        />
      </ReactFlow>
    </div>
  );
}

export function TopologyMap() {
  return (
    <ReactFlowProvider>
      <TopologyMapInner />
    </ReactFlowProvider>
  );
}
