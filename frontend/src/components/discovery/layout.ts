import ELK, { type ElkNode, type ElkExtendedEdge } from 'elkjs/lib/elk.bundled.js';
import type { Node, Edge } from '@xyflow/react';
import type { CorrelationResult } from '@/api/discovery';
import type {
  HostGroupNodeData,
  ServiceNodeData,
  ExternalNodeData,
  BatchJobNodeData,
  DependencyEdgeData,
  UnresolvedEdgeData,
} from './TopologyMap.types';
import { TECHNOLOGY_COLORS } from '@/lib/colors';

const elk = new ELK();

const SERVICE_W = 200;
const SERVICE_H = 80;
const EXTERNAL_W = 160;
const EXTERNAL_H = 56;
const BATCH_W = 180;
const BATCH_H = 64;
const HOST_PADDING_TOP = 52;
const HOST_PADDING = 24;

interface LayoutInput {
  correlationResult: CorrelationResult;
  enabledIndices: Set<number>;
  showUnresolved: boolean;
  showBatchJobs: boolean;
  getEffectiveName: (index: number) => string;
  getEffectiveType: (index: number) => string;
  highlightedServiceIndex: number | null;
  onToggle: (index: number) => void;
  onSelect: (index: number) => void;
}

interface LayoutOutput {
  nodes: Node[];
  edges: Edge[];
}

export async function computeElkLayout(input: LayoutInput): Promise<LayoutOutput> {
  const { correlationResult, enabledIndices, showUnresolved, showBatchJobs, getEffectiveName, getEffectiveType, highlightedServiceIndex, onToggle, onSelect } = input;
  const { services, dependencies, unresolved_connections, scheduled_jobs } = correlationResult;

  // Group services by hostname
  const hostMap = new Map<string, number[]>();
  services.forEach((s, i) => {
    const list = hostMap.get(s.hostname) || [];
    list.push(i);
    hostMap.set(s.hostname, list);
  });

  // Build ELK graph
  const elkChildren: ElkNode[] = [];
  const elkEdges: ElkExtendedEdge[] = [];

  // Host group nodes with service children
  for (const [hostname, indices] of hostMap) {
    const childNodes: ElkNode[] = indices.map((idx) => ({
      id: `svc-${idx}`,
      width: SERVICE_W,
      height: SERVICE_H,
    }));

    elkChildren.push({
      id: `host-${hostname}`,
      layoutOptions: {
        'elk.padding': `[top=${HOST_PADDING_TOP},left=${HOST_PADDING},bottom=${HOST_PADDING},right=${HOST_PADDING}]`,
        'elk.algorithm': 'layered',
        'elk.direction': 'DOWN',
        'elk.spacing.nodeNode': '30',
        'elk.layered.spacing.nodeNodeBetweenLayers': '60',
      },
      children: childNodes,
    });
  }

  // External nodes for unresolved connections
  const externalNodes = new Map<string, { addr: string; port: number }>();
  if (showUnresolved) {
    for (const conn of unresolved_connections) {
      const key = `${conn.remote_addr}:${conn.remote_port}`;
      if (!externalNodes.has(key)) {
        externalNodes.set(key, { addr: conn.remote_addr, port: conn.remote_port });
        elkChildren.push({
          id: `ext-${key}`,
          width: EXTERNAL_W,
          height: EXTERNAL_H,
        });
      }
    }
  }

  // Batch job nodes
  if (showBatchJobs && scheduled_jobs.length > 0) {
    scheduled_jobs.forEach((job, i) => {
      elkChildren.push({
        id: `batch-${i}`,
        width: BATCH_W,
        height: BATCH_H,
      });
    });
  }

  // Dependency edges
  for (let i = 0; i < dependencies.length; i++) {
    const dep = dependencies[i];
    if (dep.from_service_index === null || dep.from_service_index === undefined) continue;
    elkEdges.push({
      id: `dep-${i}`,
      sources: [`svc-${dep.from_service_index}`],
      targets: [`svc-${dep.to_service_index}`],
    });
  }

  // Unresolved connection edges
  if (showUnresolved) {
    unresolved_connections.forEach((conn, i) => {
      // Find service index by process name + hostname
      const svcIdx = services.findIndex(
        (s) => s.process_name === conn.from_process && s.hostname === conn.from_hostname
      );
      if (svcIdx >= 0) {
        const key = `${conn.remote_addr}:${conn.remote_port}`;
        elkEdges.push({
          id: `unres-${i}`,
          sources: [`svc-${svcIdx}`],
          targets: [`ext-${key}`],
        });
      }
    });
  }

  // Run ELK layout
  const elkGraph: ElkNode = {
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': 'DOWN',
      'elk.spacing.nodeNode': '60',
      'elk.layered.spacing.nodeNodeBetweenLayers': '100',
      'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
      'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
    },
    children: elkChildren,
    edges: elkEdges,
  };

  const layoutResult = await elk.layout(elkGraph);

  // Convert ELK output to React Flow nodes + edges
  const rfNodes: Node[] = [];
  const rfEdges: Edge[] = [];

  // Host group nodes
  for (const [hostname, indices] of hostMap) {
    const elkHost = layoutResult.children?.find((c) => c.id === `host-${hostname}`);
    if (!elkHost) continue;

    const agentId = services[indices[0]]?.agent_id || '';

    rfNodes.push({
      id: `host-${hostname}`,
      type: 'hostGroup',
      position: { x: elkHost.x || 0, y: elkHost.y || 0 },
      data: {
        hostname,
        agentId,
        serviceCount: indices.length,
      } satisfies HostGroupNodeData,
      style: { width: elkHost.width, height: elkHost.height },
      draggable: true,
      selectable: false,
    });

    // Service nodes (children of host group)
    for (const idx of indices) {
      const svc = services[idx];
      const elkSvc = elkHost.children?.find((c) => c.id === `svc-${idx}`);
      if (!elkSvc) continue;

      rfNodes.push({
        id: `svc-${idx}`,
        type: 'service',
        position: { x: elkSvc.x || 0, y: elkSvc.y || 0 },
        parentId: `host-${hostname}`,
        extent: 'parent' as const,
        data: {
          serviceIndex: idx,
          service: svc,
          label: getEffectiveName(idx),
          processName: svc.process_name,
          hostname: svc.hostname,
          ports: svc.ports,
          componentType: getEffectiveType(idx),
          commandConfidence: svc.command_suggestion?.confidence || 'low',
          enabled: enabledIndices.has(idx),
          highlighted: highlightedServiceIndex === idx,
          onToggle,
          onSelect,
        } satisfies ServiceNodeData,
      });
    }
  }

  // External nodes
  if (showUnresolved) {
    for (const [key, ext] of externalNodes) {
      const elkExt = layoutResult.children?.find((c) => c.id === `ext-${key}`);
      rfNodes.push({
        id: `ext-${key}`,
        type: 'external',
        position: { x: elkExt?.x || 0, y: elkExt?.y || 0 },
        data: {
          address: ext.addr,
          port: ext.port,
        } satisfies ExternalNodeData,
      });
    }
  }

  // Batch job nodes
  if (showBatchJobs) {
    scheduled_jobs.forEach((job, i) => {
      const elkBatch = layoutResult.children?.find((c) => c.id === `batch-${i}`);
      rfNodes.push({
        id: `batch-${i}`,
        type: 'batch',
        position: { x: elkBatch?.x || 0, y: elkBatch?.y || 0 },
        data: {
          name: job.name,
          schedule: job.schedule,
          command: job.command,
          source: job.source,
          user: job.user,
          hostname: job.hostname || 'unknown',
        } satisfies BatchJobNodeData,
      });
    });
  }

  // Dependency edges
  for (let i = 0; i < dependencies.length; i++) {
    const dep = dependencies[i];
    if (dep.from_service_index === null || dep.from_service_index === undefined) continue;
    const tech = dep.technology || 'default';
    rfEdges.push({
      id: `dep-${i}`,
      source: `svc-${dep.from_service_index}`,
      target: `svc-${dep.to_service_index}`,
      type: 'dependency',
      data: {
        technology: dep.technology,
        port: dep.remote_port,
        inferredVia: dep.inferred_via,
        configKey: dep.config_key,
        fromProcess: dep.from_process,
        toProcess: dep.to_process,
        remoteAddr: dep.remote_addr,
      } satisfies DependencyEdgeData,
      style: {
        stroke: TECHNOLOGY_COLORS[tech] || TECHNOLOGY_COLORS.default,
        strokeWidth: 2,
      },
    });
  }

  // Unresolved edges
  if (showUnresolved) {
    unresolved_connections.forEach((conn, i) => {
      const svcIdx = services.findIndex(
        (s) => s.process_name === conn.from_process && s.hostname === conn.from_hostname
      );
      if (svcIdx >= 0) {
        const key = `${conn.remote_addr}:${conn.remote_port}`;
        rfEdges.push({
          id: `unres-${i}`,
          source: `svc-${svcIdx}`,
          target: `ext-${key}`,
          type: 'unresolved',
          data: {
            fromHostname: conn.from_hostname,
            fromProcess: conn.from_process,
            remoteAddr: conn.remote_addr,
            port: conn.remote_port,
          } satisfies UnresolvedEdgeData,
          style: {
            stroke: '#94a3b8',
            strokeWidth: 1.5,
            strokeDasharray: '8 4',
          },
        });
      }
    });
  }

  return { nodes: rfNodes, edges: rfEdges };
}
