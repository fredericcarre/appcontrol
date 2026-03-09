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
import { useAgents } from '@/api/reports';
import type { AgentInfo } from './layout';

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
    dependencyMode,
    pendingDependency,
    setPendingDependency,
    addManualDependency,
    setDependencyMode,
    manualDependencies,
  } = useDiscoveryStore();

  const { data: agents } = useAgents();

  // Build agent info map for passing to layout
  const agentInfoMap = useMemo(() => {
    if (!agents) return undefined;
    const map = new Map<string, AgentInfo>();
    for (const agent of agents) {
      map.set(agent.id, {
        id: agent.id,
        hostname: agent.hostname,
        gateway_name: agent.gateway_name,
        gateway_zone: agent.gateway_zone,
        connected: agent.connected,
        gateway_connected: agent.gateway_connected,
      });
    }
    return map;
  }, [agents]);

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
    agentInfoMap,
    manualDependencies,
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

  // Handle ESC key to exit dependency creation mode
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && dependencyMode === 'create') {
        setPendingDependency(null);
        setDependencyMode('view');
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [dependencyMode, setPendingDependency, setDependencyMode]);

  const onNodeClick = useCallback(
    (_: React.MouseEvent, node: Node) => {
      if (node.type !== 'service') return;

      const idx = (node.data as { serviceIndex: number }).serviceIndex;

      // Handle dependency creation mode
      if (dependencyMode === 'create') {
        if (!pendingDependency) {
          // First click: set source
          setPendingDependency({ fromIndex: idx });
        } else if (pendingDependency.fromIndex !== idx) {
          // Second click on different node: create dependency
          addManualDependency(pendingDependency.fromIndex, idx);
          setPendingDependency(null);
        }
        return;
      }

      // Normal mode: select node
      setSelectedServiceIndex(idx);
    },
    [dependencyMode, pendingDependency, setPendingDependency, addManualDependency, setSelectedServiceIndex]
  );

  const onPaneClick = useCallback(() => {
    if (dependencyMode === 'create') {
      // Cancel pending dependency on pane click
      setPendingDependency(null);
      return;
    }
    setSelectedServiceIndex(null);
  }, [dependencyMode, setPendingDependency, setSelectedServiceIndex]);

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

  const isCreateMode = dependencyMode === 'create';

  return (
    <div className={`w-full h-full relative ${isCreateMode ? 'cursor-crosshair' : ''}`}>
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
