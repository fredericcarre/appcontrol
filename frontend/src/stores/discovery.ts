import { create } from 'zustand';
import type { CorrelationResult, TechnologyHint } from '@/api/discovery';
import type { DiscoveryPhase, ServiceEdits } from '@/components/discovery/TopologyMap.types';

// Service triage status
export type TriageStatus = 'pending' | 'include' | 'ignore';

// AI suggestion for unidentified services
export interface AISuggestion {
  technology: TechnologyHint;
  suggestedName: string;
  description: string;
  commands: {
    check?: string;
    start?: string;
    stop?: string;
  };
  confidence: 'high' | 'medium' | 'low';
}

// Enhanced agent info with gateway details
export interface EnhancedAgentInfo {
  gateway_name: string;
  gateway_zone: string;
  hostname: string;
  last_heartbeat: string | null;
}

// Manual dependency created by user
export interface ManualDependency {
  from: number;
  to: number;
}

interface DiscoveryState {
  // Phase
  phase: DiscoveryPhase;
  setPhase: (phase: DiscoveryPhase) => void;

  // Cancel flow
  cancelConfirmOpen: boolean;
  setCancelConfirmOpen: (open: boolean) => void;
  cancelDiscovery: () => void;

  // Agent selection
  selectedAgentIds: string[];
  setSelectedAgentIds: (ids: string[]) => void;
  toggleAgentId: (id: string) => void;

  // Enhanced agent info (gateway name, hostname, etc.)
  agentDetails: Map<string, EnhancedAgentInfo>;
  setAgentDetails: (details: Map<string, EnhancedAgentInfo>) => void;

  // Dependency creation mode
  dependencyMode: 'view' | 'create';
  setDependencyMode: (mode: 'view' | 'create') => void;
  pendingDependency: { fromIndex: number } | null;
  setPendingDependency: (pending: { fromIndex: number } | null) => void;
  manualDependencies: ManualDependency[];
  addManualDependency: (from: number, to: number) => void;
  removeManualDependency: (from: number, to: number) => void;

  // Site selection for app creation
  selectedSiteId: string | null;
  setSelectedSiteId: (siteId: string | null) => void;

  // Node entrance animations
  nodesAnimating: Set<number>;
  setNodesAnimating: (indices: Set<number>) => void;
  removeAnimatingNode: (index: number) => void;

  // Scan progress for Matrix animation
  scanProgress: number;
  setScanProgress: (progress: number) => void;

  // Correlation result
  correlationResult: CorrelationResult | null;
  setCorrelationResult: (result: CorrelationResult) => void;

  // Service edits (per index)
  serviceEdits: Map<number, ServiceEdits>;
  updateServiceEdit: (index: number, edits: Partial<ServiceEdits>) => void;
  getEffectiveName: (index: number) => string;
  getEffectiveType: (index: number) => string;

  // Service selection (which ones to include in app)
  enabledServiceIndices: Set<number>;
  toggleServiceEnabled: (index: number) => void;
  enableAll: () => void;
  disableAll: () => void;

  // UI selection
  selectedServiceIndex: number | null;
  setSelectedServiceIndex: (index: number | null) => void;
  highlightedServiceIndex: number | null;
  setHighlightedServiceIndex: (index: number | null) => void;

  // Triage state (new phase between scan and topology)
  serviceTriageStatus: Map<number, TriageStatus>;
  setServiceTriageStatus: (index: number, status: TriageStatus) => void;
  bulkSetTriageStatus: (indices: number[], status: TriageStatus) => void;
  aiSuggestions: Map<number, AISuggestion>;
  setAISuggestion: (index: number, suggestion: AISuggestion) => void;
  clearAISuggestions: () => void;
  getTriageCounts: () => { included: number; ignored: number; pending: number; total: number };
  getTriageProgress: () => number;
  resetTriageStatus: () => void;

  // Identified vs unidentified services
  isServiceIdentified: (index: number) => boolean;
  getUnidentifiedServices: () => number[];
  getIdentifiedServices: () => number[];

  // Filters
  showUnresolved: boolean;
  toggleShowUnresolved: () => void;
  showBatchJobs: boolean;
  toggleShowBatchJobs: () => void;
  searchQuery: string;
  setSearchQuery: (q: string) => void;

  // App creation
  appName: string;
  setAppName: (name: string) => void;
  createdAppId: string | null;
  setCreatedAppId: (id: string) => void;

  // Reset
  reset: () => void;
}

export const useDiscoveryStore = create<DiscoveryState>()((set, get) => ({
  phase: 'scan',
  setPhase: (phase) => set({ phase }),

  // Cancel flow
  cancelConfirmOpen: false,
  setCancelConfirmOpen: (open) => set({ cancelConfirmOpen: open }),
  cancelDiscovery: () => {
    set({
      phase: 'scan',
      cancelConfirmOpen: false,
      correlationResult: null,
      serviceEdits: new Map(),
      enabledServiceIndices: new Set(),
      selectedServiceIndex: null,
      highlightedServiceIndex: null,
      dependencyMode: 'view',
      pendingDependency: null,
      manualDependencies: [],
      selectedSiteId: null,
      nodesAnimating: new Set(),
      appName: '',
    });
  },

  selectedAgentIds: [],
  setSelectedAgentIds: (ids) => set({ selectedAgentIds: ids }),
  toggleAgentId: (id) =>
    set((s) => ({
      selectedAgentIds: s.selectedAgentIds.includes(id)
        ? s.selectedAgentIds.filter((a) => a !== id)
        : [...s.selectedAgentIds, id],
    })),

  // Enhanced agent info
  agentDetails: new Map(),
  setAgentDetails: (details) => set({ agentDetails: details }),

  // Dependency creation mode
  dependencyMode: 'view',
  setDependencyMode: (mode) => set({ dependencyMode: mode, pendingDependency: null }),
  pendingDependency: null,
  setPendingDependency: (pending) => set({ pendingDependency: pending }),
  manualDependencies: [],
  addManualDependency: (from, to) =>
    set((s) => {
      // Avoid duplicates
      if (s.manualDependencies.some((d) => d.from === from && d.to === to)) {
        return s;
      }
      return { manualDependencies: [...s.manualDependencies, { from, to }] };
    }),
  removeManualDependency: (from, to) =>
    set((s) => ({
      manualDependencies: s.manualDependencies.filter((d) => !(d.from === from && d.to === to)),
    })),

  // Site selection
  selectedSiteId: null,
  setSelectedSiteId: (siteId) => set({ selectedSiteId: siteId }),

  // Node entrance animations
  nodesAnimating: new Set(),
  setNodesAnimating: (indices) => set({ nodesAnimating: indices }),
  removeAnimatingNode: (index) =>
    set((s) => {
      const next = new Set(s.nodesAnimating);
      next.delete(index);
      return { nodesAnimating: next };
    }),

  // Scan progress
  scanProgress: 0,
  setScanProgress: (progress) => set({ scanProgress: progress }),

  correlationResult: null,
  setCorrelationResult: (result) => {
    const enabled = new Set<number>();
    result.services.forEach((_, i) => enabled.add(i));
    set({
      correlationResult: result,
      enabledServiceIndices: enabled,
      serviceEdits: new Map(),
      selectedServiceIndex: null,
      highlightedServiceIndex: null,
    });
  },

  serviceEdits: new Map(),
  updateServiceEdit: (index, edits) =>
    set((s) => {
      const map = new Map(s.serviceEdits);
      map.set(index, { ...map.get(index), ...edits });
      return { serviceEdits: map };
    }),
  getEffectiveName: (index) => {
    const s = get();
    return s.serviceEdits.get(index)?.name || s.correlationResult?.services[index]?.suggested_name || `service-${index}`;
  },
  getEffectiveType: (index) => {
    const s = get();
    return s.serviceEdits.get(index)?.componentType || s.correlationResult?.services[index]?.component_type || 'service';
  },

  enabledServiceIndices: new Set(),
  toggleServiceEnabled: (index) =>
    set((s) => {
      const next = new Set(s.enabledServiceIndices);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return { enabledServiceIndices: next };
    }),
  enableAll: () =>
    set((s) => {
      const all = new Set<number>();
      s.correlationResult?.services.forEach((_, i) => all.add(i));
      return { enabledServiceIndices: all };
    }),
  disableAll: () => set({ enabledServiceIndices: new Set() }),

  selectedServiceIndex: null,
  setSelectedServiceIndex: (index) => set({ selectedServiceIndex: index }),
  highlightedServiceIndex: null,
  setHighlightedServiceIndex: (index) => set({ highlightedServiceIndex: index }),

  // Triage state
  serviceTriageStatus: new Map(),
  setServiceTriageStatus: (index, status) =>
    set((s) => {
      const map = new Map(s.serviceTriageStatus);
      map.set(index, status);
      // Also update enabledServiceIndices based on triage
      const enabled = new Set(s.enabledServiceIndices);
      if (status === 'include') enabled.add(index);
      else enabled.delete(index);
      return { serviceTriageStatus: map, enabledServiceIndices: enabled };
    }),
  bulkSetTriageStatus: (indices, status) =>
    set((s) => {
      const map = new Map(s.serviceTriageStatus);
      const enabled = new Set(s.enabledServiceIndices);
      indices.forEach((i) => {
        map.set(i, status);
        if (status === 'include') enabled.add(i);
        else enabled.delete(i);
      });
      return { serviceTriageStatus: map, enabledServiceIndices: enabled };
    }),
  aiSuggestions: new Map(),
  setAISuggestion: (index, suggestion) =>
    set((s) => {
      const map = new Map(s.aiSuggestions);
      map.set(index, suggestion);
      return { aiSuggestions: map };
    }),
  clearAISuggestions: () => set({ aiSuggestions: new Map() }),
  getTriageCounts: () => {
    const s = get();
    const total = s.correlationResult?.services.length || 0;
    let included = 0;
    let ignored = 0;
    s.serviceTriageStatus.forEach((status) => {
      if (status === 'include') included++;
      else if (status === 'ignore') ignored++;
    });
    return { included, ignored, pending: total - included - ignored, total };
  },
  getTriageProgress: () => {
    const { included, ignored, total } = get().getTriageCounts();
    return total > 0 ? Math.round(((included + ignored) / total) * 100) : 0;
  },
  resetTriageStatus: () =>
    set({
      serviceTriageStatus: new Map(),
      aiSuggestions: new Map(),
    }),

  // Service identification helpers
  isServiceIdentified: (index) => {
    const s = get();
    const service = s.correlationResult?.services[index];
    return !!service?.technology_hint;
  },
  getUnidentifiedServices: () => {
    const s = get();
    const result: number[] = [];
    s.correlationResult?.services.forEach((svc, i) => {
      if (!svc.technology_hint) result.push(i);
    });
    return result;
  },
  getIdentifiedServices: () => {
    const s = get();
    const result: number[] = [];
    s.correlationResult?.services.forEach((svc, i) => {
      if (svc.technology_hint) result.push(i);
    });
    return result;
  },

  showUnresolved: true,
  toggleShowUnresolved: () => set((s) => ({ showUnresolved: !s.showUnresolved })),
  showBatchJobs: true,
  toggleShowBatchJobs: () => set((s) => ({ showBatchJobs: !s.showBatchJobs })),
  searchQuery: '',
  setSearchQuery: (q) => set({ searchQuery: q }),

  appName: '',
  setAppName: (name) => set({ appName: name }),
  createdAppId: null,
  setCreatedAppId: (id) => set({ createdAppId: id }),

  reset: () =>
    set({
      phase: 'scan',
      cancelConfirmOpen: false,
      selectedAgentIds: [],
      agentDetails: new Map(),
      dependencyMode: 'view',
      pendingDependency: null,
      manualDependencies: [],
      selectedSiteId: null,
      nodesAnimating: new Set(),
      scanProgress: 0,
      correlationResult: null,
      serviceEdits: new Map(),
      enabledServiceIndices: new Set(),
      selectedServiceIndex: null,
      highlightedServiceIndex: null,
      serviceTriageStatus: new Map(),
      aiSuggestions: new Map(),
      showUnresolved: true,
      showBatchJobs: true,
      searchQuery: '',
      appName: '',
      createdAppId: null,
    }),
}));
