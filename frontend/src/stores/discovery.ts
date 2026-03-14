import { create } from 'zustand';
import type { CorrelationResult, TechnologyHint, SystemService, DiscoveredScheduledJob } from '@/api/discovery';
import type { Application } from '@/api/apps';
import type { DiscoveryPhase, ServiceEdits, ServiceConfidence } from '@/components/discovery/TopologyMap.types';
import { classifyConfidence } from '@/components/discovery/confidence';

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

// Key for ignored auto-detected dependencies
export type IgnoredDepKey = `${number}->${number}`;

// Batch job link (jobIndex -> serviceIndex)
export interface BatchJobLink {
  jobIndex: number;
  serviceIndex: number;
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

  // Dependency mode: view, create, or delete
  dependencyMode: 'view' | 'create' | 'delete';
  setDependencyMode: (mode: 'view' | 'create' | 'delete') => void;
  pendingDependency: { fromIndex: number } | null;
  setPendingDependency: (pending: { fromIndex: number } | null) => void;
  manualDependencies: ManualDependency[];
  addManualDependency: (from: number, to: number) => void;
  removeManualDependency: (from: number, to: number) => void;
  // Ignored auto-detected dependencies
  ignoredDependencies: Set<IgnoredDepKey>;
  ignoreDependency: (from: number, to: number) => void;
  restoreDependency: (from: number, to: number) => void;
  isDependencyIgnored: (from: number, to: number) => boolean;

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

  // Confidence filters (new map-first approach)
  selectedConfidenceLevels: Set<ServiceConfidence>;
  toggleConfidenceFilter: (level: ServiceConfidence) => void;
  setConfidenceFilters: (levels: ServiceConfidence[]) => void;
  getServiceConfidence: (index: number) => ServiceConfidence;
  getConfidenceCounts: () => Record<ServiceConfidence, number>;

  // Batch job linking
  batchJobLinks: Map<number, number>; // jobIndex -> serviceIndex
  linkBatchJob: (jobIndex: number, serviceIndex: number) => void;
  unlinkBatchJob: (jobIndex: number) => void;
  getLinkedBatchJobs: (serviceIndex: number) => number[];

  // System services (Windows Services / systemd)
  addSystemServiceAsComponent: (service: SystemService) => void;

  // Existing apps as synthetic components
  addExistingAppAsComponent: (app: Application) => void;

  // Scheduled jobs as batch components
  addScheduledJobAsComponent: (job: DiscoveredScheduledJob) => void;

  // Manual component creation
  addManualComponent: (name: string, hostname: string, componentType: string) => void;

  // Filters - individual selection for batch jobs and externals
  enabledBatchJobIndices: Set<number>;
  toggleBatchJobEnabled: (index: number) => void;
  enabledExternalIndices: Set<number>;
  toggleExternalEnabled: (index: number) => void;
  // Expand/collapse state for sidebar sections
  batchJobsExpanded: boolean;
  setBatchJobsExpanded: (expanded: boolean) => void;
  externalsExpanded: boolean;
  setExternalsExpanded: (expanded: boolean) => void;
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
      ignoredDependencies: new Set(),
      selectedSiteId: null,
      nodesAnimating: new Set(),
      selectedConfidenceLevels: new Set<ServiceConfidence>(['recognized', 'likely']),
      batchJobLinks: new Map(),
      enabledBatchJobIndices: new Set(),
      enabledExternalIndices: new Set(),
      batchJobsExpanded: false,
      externalsExpanded: false,
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

  // Ignored auto-detected dependencies
  ignoredDependencies: new Set(),
  ignoreDependency: (from, to) =>
    set((s) => {
      const next = new Set(s.ignoredDependencies);
      next.add(`${from}->${to}` as IgnoredDepKey);
      return { ignoredDependencies: next };
    }),
  restoreDependency: (from, to) =>
    set((s) => {
      const next = new Set(s.ignoredDependencies);
      next.delete(`${from}->${to}` as IgnoredDepKey);
      return { ignoredDependencies: next };
    }),
  isDependencyIgnored: (from, to) => {
    const s = get();
    return s.ignoredDependencies.has(`${from}->${to}` as IgnoredDepKey);
  },

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

  // Confidence filters - default to showing recognized and likely, hiding unknown and system
  selectedConfidenceLevels: new Set<ServiceConfidence>(['recognized', 'likely']),
  toggleConfidenceFilter: (level) =>
    set((s) => {
      const next = new Set(s.selectedConfidenceLevels);
      if (next.has(level)) next.delete(level);
      else next.add(level);
      return { selectedConfidenceLevels: next };
    }),
  setConfidenceFilters: (levels) =>
    set({ selectedConfidenceLevels: new Set(levels) }),
  getServiceConfidence: (index) => {
    const s = get();
    const service = s.correlationResult?.services[index];
    if (!service) return 'unknown';
    return classifyConfidence(service);
  },
  getConfidenceCounts: () => {
    const s = get();
    const counts: Record<ServiceConfidence, number> = {
      recognized: 0,
      likely: 0,
      unknown: 0,
      system: 0,
    };
    s.correlationResult?.services.forEach((svc) => {
      const confidence = classifyConfidence(svc);
      counts[confidence]++;
    });
    return counts;
  },

  // Batch job linking
  batchJobLinks: new Map(),
  linkBatchJob: (jobIndex, serviceIndex) =>
    set((s) => {
      const map = new Map(s.batchJobLinks);
      map.set(jobIndex, serviceIndex);
      return { batchJobLinks: map };
    }),
  unlinkBatchJob: (jobIndex) =>
    set((s) => {
      const map = new Map(s.batchJobLinks);
      map.delete(jobIndex);
      return { batchJobLinks: map };
    }),
  getLinkedBatchJobs: (serviceIndex) => {
    const s = get();
    const linked: number[] = [];
    s.batchJobLinks.forEach((svcIdx, jobIdx) => {
      if (svcIdx === serviceIndex) linked.push(jobIdx);
    });
    return linked;
  },

  // Add a system service (Windows Service / systemd unit) as a component
  addSystemServiceAsComponent: (service) =>
    set((s) => {
      if (!s.correlationResult) return s;

      // Create a new correlated service from the system service
      const newService = {
        agent_id: service.agent_id,
        hostname: service.hostname,
        process_name: service.name,
        ports: [] as number[],
        port_details: [] as Array<{ port: number; address: string; pid: number | null }>,
        suggested_name: `${service.display_name}@${service.hostname}`,
        component_type: 'service',
        command_suggestion: {
          check_cmd: service.check_cmd,
          start_cmd: service.start_cmd,
          stop_cmd: service.stop_cmd,
          confidence: 'high',
          source: 'system-service',
        },
      };

      // Add to services array
      const newServices = [...s.correlationResult.services, newService];
      const newIndex = newServices.length - 1;

      // Enable the new service
      const newEnabled = new Set(s.enabledServiceIndices);
      newEnabled.add(newIndex);

      // Set service edits with commands pre-filled
      const newEdits = new Map(s.serviceEdits);
      newEdits.set(newIndex, {
        name: service.display_name || service.name,
        componentType: 'service',
        checkCmd: service.check_cmd,
        startCmd: service.start_cmd,
        stopCmd: service.stop_cmd,
      });

      return {
        correlationResult: {
          ...s.correlationResult,
          services: newServices,
        },
        enabledServiceIndices: newEnabled,
        serviceEdits: newEdits,
      };
    }),

  // Add an existing application as a synthetic component
  // This creates a "meta-component" that represents another app's aggregate state
  addExistingAppAsComponent: (app) =>
    set((s) => {
      if (!s.correlationResult) return s;

      // Create a new correlated service from the existing app
      // Component type is 'application' to distinguish it from regular services
      const newService = {
        agent_id: '', // No specific agent - this is a synthetic component
        hostname: 'aggregate',
        process_name: app.name,
        ports: [] as number[],
        port_details: [] as Array<{ port: number; address: string; pid: number | null }>,
        suggested_name: app.name,
        component_type: 'application',
        command_suggestion: {
          // Internal commands - backend interprets @app: prefix
          // and executes against the referenced app via API
          check_cmd: '@app:check',
          start_cmd: '@app:start',
          stop_cmd: '@app:stop',
          confidence: 'high',
          source: 'existing-app',
        },
        // Store the referenced app ID for later resolution
        technology_hint: {
          id: 'application',
          display_name: app.name,
          icon: 'app',
          layer: 'Application',
        },
        // Additional metadata for the referenced app
        matched_service: app.id, // Store app ID here for reference
      };

      // Add to services array
      const newServices = [...s.correlationResult.services, newService];
      const newIndex = newServices.length - 1;

      // Enable the new service
      const newEnabled = new Set(s.enabledServiceIndices);
      newEnabled.add(newIndex);

      // Set service edits with metadata
      // Commands use @app: prefix - backend interprets these internally
      const newEdits = new Map(s.serviceEdits);
      newEdits.set(newIndex, {
        name: app.name,
        componentType: 'application',
        checkCmd: '@app:check',
        startCmd: '@app:start',
        stopCmd: '@app:stop',
        // Store reference to the app - backend uses this to resolve commands
        referencedAppId: app.id,
        referencedAppName: app.name,
      });

      return {
        correlationResult: {
          ...s.correlationResult,
          services: newServices,
        },
        enabledServiceIndices: newEnabled,
        serviceEdits: newEdits,
      };
    }),

  // Add a scheduled job (cron/Task Scheduler) as a batch component
  addScheduledJobAsComponent: (job) =>
    set((s) => {
      if (!s.correlationResult) return s;

      // Detect if Windows (schtasks) or Unix (cron)
      const isWindows = job.source === 'task-scheduler' || job.source === 'schtasks';

      // Generate commands based on scheduler type
      const checkCmd = isWindows
        ? `schtasks /Query /TN "${job.name}" /FO LIST | findstr "Status"`
        : `systemctl is-active ${job.name}.timer 2>/dev/null || crontab -l | grep -q "${job.name}"`;

      const startCmd = isWindows
        ? `schtasks /Run /TN "${job.name}"`
        : undefined; // Cron jobs run on schedule, can't "start" them

      const stopCmd = isWindows
        ? `schtasks /End /TN "${job.name}"`
        : undefined;

      const newService = {
        agent_id: '', // Will need to be set based on hostname
        hostname: job.hostname || 'unknown',
        process_name: job.name,
        ports: [] as number[],
        port_details: [] as Array<{ port: number; address: string; pid: number | null }>,
        suggested_name: job.name,
        component_type: 'batch',
        command_suggestion: {
          check_cmd: checkCmd,
          start_cmd: startCmd,
          stop_cmd: stopCmd,
          confidence: 'medium',
          source: job.source,
        },
        technology_hint: {
          id: 'scheduler',
          display_name: job.name,
          icon: 'scheduler',
          layer: 'Scheduler',
        },
      };

      const newServices = [...s.correlationResult.services, newService];
      const newIndex = newServices.length - 1;

      const newEnabled = new Set(s.enabledServiceIndices);
      newEnabled.add(newIndex);

      const newEdits = new Map(s.serviceEdits);
      newEdits.set(newIndex, {
        name: job.name,
        componentType: 'batch',
        checkCmd: checkCmd,
        startCmd: startCmd,
        stopCmd: stopCmd,
      });

      return {
        correlationResult: {
          ...s.correlationResult,
          services: newServices,
        },
        enabledServiceIndices: newEnabled,
        serviceEdits: newEdits,
      };
    }),

  // Add a manually created component (not from discovery)
  addManualComponent: (name, hostname, componentType) =>
    set((s) => {
      if (!s.correlationResult) return s;

      // Create a new manual service
      const newService = {
        agent_id: '', // Manual - no agent
        hostname: hostname || 'manual',
        process_name: name,
        ports: [] as number[],
        port_details: [] as Array<{ port: number; address: string; pid: number | null }>,
        suggested_name: name,
        component_type: componentType,
        command_suggestion: undefined,
        technology_hint: undefined,
      };

      const newServices = [...s.correlationResult.services, newService];
      const newIndex = newServices.length - 1;

      const newEnabled = new Set(s.enabledServiceIndices);
      newEnabled.add(newIndex);

      const newEdits = new Map(s.serviceEdits);
      newEdits.set(newIndex, {
        name,
        componentType,
      });

      return {
        correlationResult: {
          ...s.correlationResult,
          services: newServices,
        },
        enabledServiceIndices: newEnabled,
        serviceEdits: newEdits,
      };
    }),

  // Individual selection for batch jobs and externals (empty by default)
  enabledBatchJobIndices: new Set(),
  toggleBatchJobEnabled: (index) =>
    set((s) => {
      const next = new Set(s.enabledBatchJobIndices);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return { enabledBatchJobIndices: next };
    }),
  enabledExternalIndices: new Set(),
  toggleExternalEnabled: (index) =>
    set((s) => {
      const next = new Set(s.enabledExternalIndices);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return { enabledExternalIndices: next };
    }),
  // Expand/collapse state for sidebar sections
  batchJobsExpanded: false,
  setBatchJobsExpanded: (expanded) => set({ batchJobsExpanded: expanded }),
  externalsExpanded: false,
  setExternalsExpanded: (expanded) => set({ externalsExpanded: expanded }),
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
      ignoredDependencies: new Set(),
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
      selectedConfidenceLevels: new Set<ServiceConfidence>(['recognized', 'likely']),
      batchJobLinks: new Map(),
      enabledBatchJobIndices: new Set(),
      enabledExternalIndices: new Set(),
      batchJobsExpanded: false,
      externalsExpanded: false,
      searchQuery: '',
      appName: '',
      createdAppId: null,
    }),
}));
