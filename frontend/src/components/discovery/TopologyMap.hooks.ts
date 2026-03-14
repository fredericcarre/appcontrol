import { useState, useEffect, useRef, useCallback } from 'react';
import type { Node, Edge } from '@xyflow/react';
import { computeElkLayout, type AgentInfo, type ManualDependency } from './layout';
import type { CorrelationResult } from '@/api/discovery';

interface UseTopologyLayoutInput {
  correlationResult: CorrelationResult | null;
  enabledIndices: Set<number>;
  enabledBatchJobIndices: Set<number>;
  enabledExternalIndices: Set<number>;
  ignoredDependencies: Set<string>;
  getEffectiveName: (index: number) => string;
  getEffectiveType: (index: number) => string;
  highlightedServiceIndex: number | null;
  onToggle: (index: number) => void;
  onSelect: (index: number) => void;
  agentInfoMap?: Map<string, AgentInfo>;
  manualDependencies?: ManualDependency[];
}

interface UseTopologyLayoutOutput {
  nodes: Node[];
  edges: Edge[];
  isLayouting: boolean;
  reLayout: () => void;
}

export function useTopologyLayout(input: UseTopologyLayoutInput): UseTopologyLayoutOutput {
  const [nodes, setNodes] = useState<Node[]>([]);
  const [edges, setEdges] = useState<Edge[]>([]);
  const [isLayouting, setIsLayouting] = useState(false);
  const layoutIdRef = useRef(0);

  const {
    correlationResult,
    enabledIndices,
    enabledBatchJobIndices,
    enabledExternalIndices,
    ignoredDependencies,
    getEffectiveName,
    getEffectiveType,
    highlightedServiceIndex,
    onToggle,
    onSelect,
    agentInfoMap,
    manualDependencies,
  } = input;

  const runLayout = useCallback(async () => {
    if (!correlationResult || correlationResult.services.length === 0) {
      setNodes([]);
      setEdges([]);
      return;
    }

    const layoutId = ++layoutIdRef.current;
    setIsLayouting(true);

    try {
      const result = await computeElkLayout({
        correlationResult,
        enabledIndices,
        enabledBatchJobIndices,
        enabledExternalIndices,
        ignoredDependencies,
        getEffectiveName,
        getEffectiveType,
        highlightedServiceIndex,
        onToggle,
        onSelect,
        agentInfoMap,
        manualDependencies,
      });

      // Only apply if this is still the latest layout request
      if (layoutId === layoutIdRef.current) {
        setNodes(result.nodes);
        setEdges(result.edges);
      }
    } catch (err) {
      console.error('ELK layout failed:', err);
    } finally {
      if (layoutId === layoutIdRef.current) {
        setIsLayouting(false);
      }
    }
  }, [correlationResult, enabledIndices, enabledBatchJobIndices, enabledExternalIndices, ignoredDependencies, getEffectiveName, getEffectiveType, highlightedServiceIndex, onToggle, onSelect, agentInfoMap, manualDependencies]);

  // Run layout on correlation result change or filter change
  useEffect(() => {
    const timer = setTimeout(runLayout, 100);
    return () => clearTimeout(timer);
  }, [runLayout]);

  return { nodes, edges, isLayouting, reLayout: runLayout };
}
