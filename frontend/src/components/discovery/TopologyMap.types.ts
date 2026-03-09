import type { CorrelationResult, CorrelatedService, CommandSuggestion, DiscoveredConfigFile, DiscoveredLogFile } from '@/api/discovery';

// ---------------------------------------------------------------------------
// React Flow node data types
// ---------------------------------------------------------------------------

export interface HostGroupNodeData {
  hostname: string;
  agentId: string;
  serviceCount: number;
  gatewayName?: string | null;
  gatewayZone?: string | null;
  gatewayConnected?: boolean;
  agentConnected?: boolean;
  [key: string]: unknown;
}

export interface ServiceNodeData {
  serviceIndex: number;
  service: CorrelatedService;
  label: string;
  processName: string;
  hostname: string;
  ports: number[];
  componentType: string;
  commandConfidence: string;
  enabled: boolean;
  highlighted: boolean;
  onToggle: (index: number) => void;
  onSelect: (index: number) => void;
  [key: string]: unknown;
}

export interface ExternalNodeData {
  address: string;
  port: number;
  technology?: string;
  [key: string]: unknown;
}

export interface BatchJobNodeData {
  name: string;
  schedule: string;
  command: string;
  source: string;
  user: string;
  hostname: string;
  [key: string]: unknown;
}

// ---------------------------------------------------------------------------
// React Flow edge data types
// ---------------------------------------------------------------------------

export interface DependencyEdgeData {
  technology?: string;
  port: number;
  inferredVia: string;
  configKey?: string;
  fromProcess: string;
  toProcess: string;
  remoteAddr: string;
  [key: string]: unknown;
}

export interface UnresolvedEdgeData {
  fromHostname: string;
  fromProcess: string;
  remoteAddr: string;
  port: number;
  [key: string]: unknown;
}

// ---------------------------------------------------------------------------
// Store types
// ---------------------------------------------------------------------------

export type DiscoveryPhase = 'scan' | 'topology' | 'done';

export interface ServiceEdits {
  name?: string;
  componentType?: string;
  checkCmd?: string;
  startCmd?: string;
  stopCmd?: string;
  restartCmd?: string;
}

export { type CorrelationResult, type CorrelatedService, type CommandSuggestion, type DiscoveredConfigFile, type DiscoveredLogFile };
