import { useState, useCallback } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from '@/components/ui/table';
import {
  Radar,
  Network,
  CheckCircle,
  AlertCircle,
  Loader2,
  Server,
  ArrowRight,
  ChevronRight,
  Plus,
  Eye,
  AlertTriangle,
  FileText,
  ScrollText,
  Clock,
  Terminal,
  Shield,
} from 'lucide-react';
import {
  useDiscoveryReports,
  useDiscoveryReport,
  useDiscoveryDrafts,
  useDiscoveryDraft,
  useTriggerAllScans,
  useCorrelate,
  useCreateDraft,
  useApplyDraft,
  type CorrelatedService,
  type CorrelatedDependency,
  type UnresolvedConnection,
  type DiscoveryReportDetail,
  type DiscoveredScheduledJob,
  type CommandSuggestion,
} from '@/api/discovery';
import { useAgents, type Agent } from '@/api/reports';

// ---------------------------------------------------------------------------
// Wizard Steps
// ---------------------------------------------------------------------------

type WizardStep = 'collect' | 'review' | 'correlate' | 'configure' | 'drafts';

const STEPS: { key: WizardStep; label: string; description: string }[] = [
  { key: 'collect', label: '1. Collect', description: 'Scan agents' },
  { key: 'review', label: '2. Review', description: 'View raw data' },
  { key: 'correlate', label: '3. Correlate', description: 'Cross-host analysis' },
  { key: 'configure', label: '4. Configure', description: 'Commands & validate' },
  { key: 'drafts', label: '5. Drafts', description: 'Apply to map' },
];

// ---------------------------------------------------------------------------
// Main Page
// ---------------------------------------------------------------------------

export function DiscoveryPage() {
  const [step, setStep] = useState<WizardStep>('collect');
  const [selectedAgents, setSelectedAgents] = useState<string[]>([]);
  const [selectedReportId, setSelectedReportId] = useState<string>();

  // Correlation state — kept between steps
  const [correlationServices, setCorrelationServices] = useState<CorrelatedService[]>([]);
  const [correlationDeps, setCorrelationDeps] = useState<CorrelatedDependency[]>([]);
  const [unresolvedConns, setUnresolvedConns] = useState<UnresolvedConnection[]>([]);
  const [scheduledJobs, setScheduledJobs] = useState<DiscoveredScheduledJob[]>([]);

  // Configure step state
  const [appName, setAppName] = useState('');
  const [editedServices, setEditedServices] = useState<CorrelatedService[]>([]);
  const [enabledServiceIndices, setEnabledServiceIndices] = useState<Set<number>>(new Set());
  const [enabledDepIndices, setEnabledDepIndices] = useState<Set<number>>(new Set());

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold flex items-center gap-2">
          <Radar className="h-6 w-6" />
          Discovery
        </h1>
        <p className="text-muted-foreground mt-1">
          Discover processes running on your servers and build application maps step by step.
        </p>
      </div>

      {/* Wizard step indicators */}
      <div className="flex items-center gap-1">
        {STEPS.map((s, i) => (
          <div key={s.key} className="flex items-center">
            <button
              onClick={() => setStep(s.key)}
              className={`flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-colors ${
                step === s.key
                  ? 'bg-primary text-primary-foreground'
                  : 'bg-muted text-muted-foreground hover:bg-accent'
              }`}
            >
              <span className="font-medium">{s.label}</span>
              <span className="hidden md:inline text-xs opacity-80">{s.description}</span>
            </button>
            {i < STEPS.length - 1 && <ChevronRight className="h-4 w-4 text-muted-foreground mx-1" />}
          </div>
        ))}
      </div>

      {/* Step content */}
      {step === 'collect' && (
        <CollectStep
          selectedAgents={selectedAgents}
          setSelectedAgents={setSelectedAgents}
          onNext={() => setStep('review')}
        />
      )}
      {step === 'review' && (
        <ReviewStep
          selectedReportId={selectedReportId}
          setSelectedReportId={setSelectedReportId}
          onNext={() => setStep('correlate')}
        />
      )}
      {step === 'correlate' && (
        <CorrelateStep
          selectedAgents={selectedAgents}
          onCorrelationDone={(services, deps, unresolved, jobs) => {
            setCorrelationServices(services);
            setCorrelationDeps(deps);
            setUnresolvedConns(unresolved);
            setScheduledJobs(jobs);
            // Pre-select all services and deps
            setEditedServices([...services]);
            setEnabledServiceIndices(new Set(services.map((_, i) => i)));
            setEnabledDepIndices(new Set(deps.map((_, i) => i)));
            setStep('configure');
          }}
        />
      )}
      {step === 'configure' && (
        <ConfigureStep
          services={correlationServices}
          dependencies={correlationDeps}
          unresolvedConns={unresolvedConns}
          scheduledJobs={scheduledJobs}
          appName={appName}
          setAppName={setAppName}
          editedServices={editedServices}
          setEditedServices={setEditedServices}
          enabledServiceIndices={enabledServiceIndices}
          setEnabledServiceIndices={setEnabledServiceIndices}
          enabledDepIndices={enabledDepIndices}
          setEnabledDepIndices={setEnabledDepIndices}
          onDraftCreated={() => setStep('drafts')}
        />
      )}
      {step === 'drafts' && <DraftsStep />}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Step 1: Collect — trigger scans
// ---------------------------------------------------------------------------

function CollectStep({
  selectedAgents,
  setSelectedAgents,
  onNext,
}: {
  selectedAgents: string[];
  setSelectedAgents: (v: string[]) => void;
  onNext: () => void;
}) {
  const { data: agentsData } = useAgents();
  const { data: reports } = useDiscoveryReports();
  const triggerAll = useTriggerAllScans();

  const agents: Agent[] = Array.isArray(agentsData)
    ? agentsData
    : (agentsData as unknown as { agents?: Agent[] })?.agents || [];

  const agentIdsWithReports = new Set(reports?.map(r => r.agent_id) || []);

  const toggleAgent = (id: string) => {
    setSelectedAgents(
      selectedAgents.includes(id)
        ? selectedAgents.filter(a => a !== id)
        : [...selectedAgents, id]
    );
  };

  const selectAll = () => setSelectedAgents(agents.map(a => a.id));

  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <Card>
        <CardContent className="p-6 space-y-4">
          <h3 className="font-semibold text-lg">Agents</h3>
          <p className="text-sm text-muted-foreground">
            Select the servers to include in your discovery scope, then trigger a scan.
          </p>

          <div className="flex gap-2">
            <button
              onClick={() => triggerAll.mutate()}
              disabled={triggerAll.isPending}
              className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              {triggerAll.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Radar className="h-4 w-4" />
              )}
              Scan All Agents
            </button>
            <button onClick={selectAll} className="rounded-md border px-3 py-2 text-sm hover:bg-accent">
              Select All
            </button>
          </div>

          {triggerAll.isSuccess && (
            <div className="text-sm text-green-700 bg-green-50 border border-green-200 rounded-md p-2">
              Scan triggered on {triggerAll.data.agents_sent}/{triggerAll.data.agents_targeted} agents. Reports will arrive in a few seconds.
            </div>
          )}

          <div className="space-y-2 max-h-64 overflow-y-auto">
            {agents.map(agent => (
              <label
                key={agent.id}
                className={`flex items-center gap-3 p-3 rounded-md border cursor-pointer transition-colors ${
                  selectedAgents.includes(agent.id) ? 'border-primary bg-primary/5' : 'border-border hover:bg-accent'
                }`}
              >
                <input
                  type="checkbox"
                  checked={selectedAgents.includes(agent.id)}
                  onChange={() => toggleAgent(agent.id)}
                  className="rounded"
                />
                <Server className="h-4 w-4 text-muted-foreground" />
                <span className="font-medium">{agent.hostname || agent.id.slice(0, 8)}</span>
                {agentIdsWithReports.has(agent.id) && (
                  <Badge variant="running" className="ml-auto text-xs">Has data</Badge>
                )}
              </label>
            ))}
          </div>

          <button
            onClick={onNext}
            disabled={selectedAgents.length === 0}
            className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            Next: Review Data <ChevronRight className="h-4 w-4" />
          </button>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="p-6 space-y-3">
          <h3 className="font-semibold text-lg">Recent Reports</h3>
          <div className="space-y-2 max-h-80 overflow-y-auto">
            {!reports?.length ? (
              <p className="text-sm text-muted-foreground py-4 text-center">No reports yet</p>
            ) : (
              reports.slice(0, 20).map(r => (
                <div key={r.id} className="flex items-center justify-between p-2 border rounded-md text-sm">
                  <div className="flex items-center gap-2">
                    <Server className="h-3 w-3 text-muted-foreground" />
                    <span className="font-medium">{r.hostname}</span>
                  </div>
                  <span className="text-muted-foreground text-xs">{new Date(r.scanned_at).toLocaleString()}</span>
                </div>
              ))
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Step 2: Review — inspect raw data per host
// ---------------------------------------------------------------------------

function ReviewStep({
  selectedReportId,
  setSelectedReportId,
  onNext,
}: {
  selectedReportId?: string;
  setSelectedReportId: (id: string) => void;
  onNext: () => void;
}) {
  const { data: reports } = useDiscoveryReports();
  const { data: report } = useDiscoveryReport(selectedReportId);

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <p className="text-sm text-muted-foreground">
          Click a report to inspect processes, listeners, connections, and detected configs on that host.
        </p>
        <button onClick={onNext} className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90">
          Next: Cross-host Analysis <ChevronRight className="h-4 w-4" />
        </button>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        {/* Report list */}
        <Card>
          <CardContent className="p-0">
            <div className="max-h-96 overflow-y-auto">
              {reports?.map(r => (
                <button
                  key={r.id}
                  onClick={() => setSelectedReportId(r.id)}
                  className={`w-full text-left p-3 border-b text-sm hover:bg-accent transition-colors ${
                    selectedReportId === r.id ? 'bg-accent' : ''
                  }`}
                >
                  <div className="font-medium">{r.hostname}</div>
                  <div className="text-xs text-muted-foreground">{new Date(r.scanned_at).toLocaleString()}</div>
                </button>
              ))}
            </div>
          </CardContent>
        </Card>

        {/* Report detail */}
        <div className="lg:col-span-2">
          {report ? <ReportDetailPanel report={report} /> : (
            <Card>
              <CardContent className="p-12 text-center text-muted-foreground">
                <Eye className="h-8 w-8 mx-auto mb-2 opacity-50" />
                Select a report to inspect
              </CardContent>
            </Card>
          )}
        </div>
      </div>
    </div>
  );
}

function ReportDetailPanel({ report }: { report: DiscoveryReportDetail }) {
  const { processes = [], listeners = [], connections = [], services = [], scheduled_jobs: scheduledJobs = [] } = report.report;
  const appProcesses = processes.filter(p => p.listening_ports?.length > 0);

  return (
    <Card>
      <CardContent className="p-4 space-y-4">
        <div className="flex items-center justify-between">
          <h3 className="font-semibold">{report.hostname}</h3>
          <div className="flex gap-4 text-sm text-muted-foreground">
            <span>{processes.length} processes</span>
            <span>{listeners.length} listeners</span>
            <span>{connections.length} connections</span>
            <span>{services.length} services</span>
            {scheduledJobs.length > 0 && <span>{scheduledJobs.length} jobs</span>}
          </div>
        </div>

        {/* Application processes (with listening ports) */}
        {appProcesses.length > 0 && (
          <div>
            <h4 className="text-sm font-semibold mb-1">Application Processes</h4>
            <div className="border rounded-md max-h-40 overflow-y-auto">
              <Table>
                <TableHeader><TableRow>
                  <TableHead>Process</TableHead><TableHead>PID</TableHead><TableHead>Ports</TableHead><TableHead>Service</TableHead><TableHead>User</TableHead>
                </TableRow></TableHeader>
                <TableBody>
                  {appProcesses.map((p) => (
                    <TableRow key={p.pid}>
                      <TableCell className="font-medium">{p.name}</TableCell>
                      <TableCell className="font-mono text-xs">{p.pid}</TableCell>
                      <TableCell>
                        <div className="flex gap-1 flex-wrap">
                          {p.listening_ports.map((port: number) => (
                            <Badge key={port} variant="secondary" className="text-xs font-mono">{port}</Badge>
                          ))}
                        </div>
                      </TableCell>
                      <TableCell className="text-xs">{p.matched_service || '-'}</TableCell>
                      <TableCell className="text-xs">{p.user}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </div>
        )}

        {/* Listeners */}
        <div>
          <h4 className="text-sm font-semibold mb-1">TCP Listeners</h4>
          <div className="border rounded-md max-h-40 overflow-y-auto">
            <Table>
              <TableHeader><TableRow>
                <TableHead className="w-20">Port</TableHead><TableHead>Process</TableHead><TableHead>PID</TableHead><TableHead>Bind</TableHead>
              </TableRow></TableHeader>
              <TableBody>
                {listeners.map((l, i) => (
                  <TableRow key={i}>
                    <TableCell className="font-mono font-semibold">{l.port}</TableCell>
                    <TableCell>{l.process_name || '-'}</TableCell>
                    <TableCell className="font-mono text-xs">{l.pid || '-'}</TableCell>
                    <TableCell className="text-muted-foreground text-xs">{l.address}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </div>

        {/* Outbound connections */}
        {connections.length > 0 && (
          <div>
            <h4 className="text-sm font-semibold mb-1">Outbound Connections ({connections.length})</h4>
            <div className="border rounded-md max-h-40 overflow-y-auto">
              <Table>
                <TableHeader><TableRow>
                  <TableHead>Process</TableHead><TableHead>Remote</TableHead><TableHead>Local Port</TableHead>
                </TableRow></TableHeader>
                <TableBody>
                  {connections.slice(0, 50).map((c, i) => (
                    <TableRow key={i}>
                      <TableCell>{c.process_name || '-'}</TableCell>
                      <TableCell className="font-mono text-xs">{c.remote_addr}:{c.remote_port}</TableCell>
                      <TableCell className="font-mono text-xs">{c.local_port}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </div>
        )}

        {/* Scheduled Jobs */}
        {scheduledJobs.length > 0 && (
          <div>
            <h4 className="text-sm font-semibold mb-1 flex items-center gap-1">
              <Clock className="h-3.5 w-3.5" /> Scheduled Jobs ({scheduledJobs.length})
            </h4>
            <div className="border rounded-md max-h-32 overflow-y-auto">
              <Table>
                <TableHeader><TableRow>
                  <TableHead>Name</TableHead><TableHead>Schedule</TableHead><TableHead>Command</TableHead><TableHead>User</TableHead>
                </TableRow></TableHeader>
                <TableBody>
                  {scheduledJobs.map((j: DiscoveredScheduledJob, i: number) => (
                    <TableRow key={i}>
                      <TableCell className="font-medium text-xs">{j.name}</TableCell>
                      <TableCell className="font-mono text-xs">{j.schedule}</TableCell>
                      <TableCell className="font-mono text-xs truncate max-w-[200px]">{j.command}</TableCell>
                      <TableCell className="text-xs">{j.user}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Step 3: Correlate — cross-host analysis
// ---------------------------------------------------------------------------

function CorrelateStep({
  selectedAgents,
  onCorrelationDone,
}: {
  selectedAgents: string[];
  onCorrelationDone: (services: CorrelatedService[], deps: CorrelatedDependency[], unresolved: UnresolvedConnection[], jobs: DiscoveredScheduledJob[]) => void;
}) {
  const correlate = useCorrelate();
  const { data: reports } = useDiscoveryReports();

  // If no agents selected, use all agents with reports
  const agentIds = selectedAgents.length > 0
    ? selectedAgents
    : [...new Set(reports?.map(r => r.agent_id) || [])];

  const runCorrelation = () => {
    correlate.mutate({ agent_ids: agentIds }, {
      onSuccess: (result) => {
        onCorrelationDone(result.services, result.dependencies, result.unresolved_connections, result.scheduled_jobs || []);
      },
    });
  };

  return (
    <Card>
      <CardContent className="p-6 space-y-4">
        <h3 className="font-semibold text-lg flex items-center gap-2">
          <Network className="h-5 w-5" />
          Cross-Host Analysis
        </h3>
        <p className="text-sm text-muted-foreground">
          The correlation engine will analyze scan data from {agentIds.length} agent(s), group processes
          by service, identify TCP connections and config-based dependencies between hosts,
          suggest operational commands, and detect scheduled jobs.
        </p>
        <p className="text-sm text-muted-foreground">
          This is not magic — you will review and adjust the results in the next step.
        </p>

        <button
          onClick={runCorrelation}
          disabled={correlate.isPending || agentIds.length === 0}
          className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
        >
          {correlate.isPending ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Network className="h-4 w-4" />
          )}
          Run Correlation
        </button>

        {correlate.isError && (
          <div className="rounded-md bg-red-50 border border-red-200 p-3 text-sm text-red-800">
            <AlertCircle className="h-4 w-4 inline mr-2" />
            {(correlate.error as Error)?.message || 'Correlation failed'}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Confidence badge helper
// ---------------------------------------------------------------------------

function ConfidenceBadge({ confidence }: { confidence?: string }) {
  switch (confidence) {
    case 'high':
      return <Badge variant="running" className="text-xs"><Shield className="h-3 w-3 mr-1" />high</Badge>;
    case 'medium':
      return <Badge variant="degraded" className="text-xs">medium</Badge>;
    case 'low':
      return <Badge variant="stopped" className="text-xs">low</Badge>;
    default:
      return null;
  }
}

// ---------------------------------------------------------------------------
// Step 4: Configure — commands, configs, logs, batch jobs
// ---------------------------------------------------------------------------

function ConfigureStep({
  services,
  dependencies,
  unresolvedConns,
  scheduledJobs,
  appName,
  setAppName,
  editedServices,
  setEditedServices,
  enabledServiceIndices,
  setEnabledServiceIndices,
  enabledDepIndices,
  setEnabledDepIndices,
  onDraftCreated,
}: {
  services: CorrelatedService[];
  dependencies: CorrelatedDependency[];
  unresolvedConns: UnresolvedConnection[];
  scheduledJobs: DiscoveredScheduledJob[];
  appName: string;
  setAppName: (v: string) => void;
  editedServices: CorrelatedService[];
  setEditedServices: (v: CorrelatedService[]) => void;
  enabledServiceIndices: Set<number>;
  setEnabledServiceIndices: (v: Set<number>) => void;
  enabledDepIndices: Set<number>;
  setEnabledDepIndices: (v: Set<number>) => void;
  onDraftCreated: () => void;
}) {
  const createDraft = useCreateDraft();
  const [expandedService, setExpandedService] = useState<number | null>(null);

  const toggleService = useCallback((idx: number) => {
    const next = new Set(enabledServiceIndices);
    if (next.has(idx)) {
      next.delete(idx);
      const nextDeps = new Set(enabledDepIndices);
      dependencies.forEach((d, di) => {
        if (d.from_service_index === idx || d.to_service_index === idx) {
          nextDeps.delete(di);
        }
      });
      setEnabledDepIndices(nextDeps);
    } else {
      next.add(idx);
    }
    setEnabledServiceIndices(next);
  }, [enabledServiceIndices, enabledDepIndices, dependencies, setEnabledServiceIndices, setEnabledDepIndices]);

  const toggleDep = useCallback((idx: number) => {
    const next = new Set(enabledDepIndices);
    if (next.has(idx)) next.delete(idx);
    else next.add(idx);
    setEnabledDepIndices(next);
  }, [enabledDepIndices, setEnabledDepIndices]);

  const updateServiceName = useCallback((idx: number, name: string) => {
    const copy = [...editedServices];
    copy[idx] = { ...copy[idx], suggested_name: name };
    setEditedServices(copy);
  }, [editedServices, setEditedServices]);

  const updateServiceType = useCallback((idx: number, type: string) => {
    const copy = [...editedServices];
    copy[idx] = { ...copy[idx], component_type: type };
    setEditedServices(copy);
  }, [editedServices, setEditedServices]);

  const updateServiceCommand = useCallback((idx: number, field: keyof CommandSuggestion & string, value: string) => {
    const copy = [...editedServices];
    const current = copy[idx].command_suggestion || { check_cmd: '', confidence: 'low', source: 'manual' };
    copy[idx] = {
      ...copy[idx],
      command_suggestion: { ...current, [field]: value || undefined },
    };
    setEditedServices(copy);
  }, [editedServices, setEditedServices]);

  const handleCreateDraft = () => {
    const enabledServices = editedServices.filter((_, i) => enabledServiceIndices.has(i));
    const tempIds = new Map<number, string>();
    enabledServices.forEach((s, i) => {
      const origIdx = editedServices.indexOf(s);
      tempIds.set(origIdx, `svc-${i}`);
    });

    const components = enabledServices.map((s, i) => ({
      temp_id: `svc-${i}`,
      name: s.suggested_name,
      process_name: s.process_name,
      host: s.hostname,
      agent_id: s.agent_id,
      listening_ports: s.ports,
      component_type: s.component_type,
      check_cmd: s.command_suggestion?.check_cmd,
      start_cmd: s.command_suggestion?.start_cmd,
      stop_cmd: s.command_suggestion?.stop_cmd,
      restart_cmd: s.command_suggestion?.restart_cmd,
      command_confidence: s.command_suggestion?.confidence || 'low',
      command_source: s.command_suggestion?.source || 'process',
      config_files: s.config_files,
      log_files: s.log_files,
      matched_service: s.matched_service,
    }));

    const deps = dependencies
      .filter((_, i) => enabledDepIndices.has(i))
      .filter(d => {
        const fromOk = d.from_service_index === null || tempIds.has(d.from_service_index);
        const toOk = tempIds.has(d.to_service_index);
        return fromOk && toOk;
      })
      .map(d => ({
        from_temp_id: d.from_service_index !== null ? (tempIds.get(d.from_service_index) || '') : '',
        to_temp_id: tempIds.get(d.to_service_index) || '',
        inferred_via: d.inferred_via,
      }))
      .filter(d => d.from_temp_id && d.to_temp_id);

    createDraft.mutate({ name: appName, components, dependencies: deps }, {
      onSuccess: () => onDraftCreated(),
    });
  };

  return (
    <div className="space-y-6">
      {/* App name */}
      <Card>
        <CardContent className="p-6 space-y-3">
          <h3 className="font-semibold text-lg">Application Name</h3>
          <input
            type="text"
            value={appName}
            onChange={(e) => setAppName(e.target.value)}
            placeholder="e.g. order-processing, payment-stack"
            className="w-full max-w-md rounded-md border border-input bg-background px-3 py-2 text-sm"
          />
        </CardContent>
      </Card>

      {/* Components — with commands, configs, logs */}
      <Card>
        <CardContent className="p-6 space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="font-semibold text-lg flex items-center gap-2">
              <Terminal className="h-5 w-5" />
              Discovered Components ({enabledServiceIndices.size}/{services.length})
            </h3>
            <p className="text-xs text-muted-foreground">Click a row to edit commands and view details</p>
          </div>
          <div className="border rounded-md">
            <Table>
              <TableHeader><TableRow>
                <TableHead className="w-10"></TableHead>
                <TableHead>Name</TableHead>
                <TableHead>Process</TableHead>
                <TableHead>Host</TableHead>
                <TableHead>Ports</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>Commands</TableHead>
              </TableRow></TableHeader>
              <TableBody>
                {editedServices.map((s, i) => (
                  <>
                    <TableRow
                      key={`row-${i}`}
                      className={`cursor-pointer ${!enabledServiceIndices.has(i) ? 'opacity-40' : ''} ${expandedService === i ? 'bg-accent/50' : ''}`}
                      onClick={() => setExpandedService(expandedService === i ? null : i)}
                    >
                      <TableCell onClick={(e) => e.stopPropagation()}>
                        <input
                          type="checkbox"
                          checked={enabledServiceIndices.has(i)}
                          onChange={() => toggleService(i)}
                          className="rounded"
                        />
                      </TableCell>
                      <TableCell onClick={(e) => e.stopPropagation()}>
                        <input
                          type="text"
                          value={s.suggested_name}
                          onChange={(e) => updateServiceName(i, e.target.value)}
                          className="w-full min-w-[160px] rounded border border-input bg-background px-2 py-1 text-sm"
                          disabled={!enabledServiceIndices.has(i)}
                        />
                      </TableCell>
                      <TableCell className="text-sm">{s.process_name}</TableCell>
                      <TableCell className="text-sm font-mono">{s.hostname}</TableCell>
                      <TableCell>
                        <div className="flex gap-1 flex-wrap">
                          {s.ports.map(p => (
                            <Badge key={p} variant="secondary" className="text-xs font-mono">{p}</Badge>
                          ))}
                        </div>
                      </TableCell>
                      <TableCell onClick={(e) => e.stopPropagation()}>
                        <select
                          value={s.component_type}
                          onChange={(e) => updateServiceType(i, e.target.value)}
                          disabled={!enabledServiceIndices.has(i)}
                          className="rounded border border-input bg-background px-2 py-1 text-xs"
                        >
                          <option value="service">service</option>
                          <option value="database">database</option>
                          <option value="cache">cache</option>
                          <option value="queue">queue</option>
                          <option value="proxy">proxy</option>
                          <option value="web">web</option>
                          <option value="search">search</option>
                          <option value="batch">batch</option>
                        </select>
                      </TableCell>
                      <TableCell>
                        {s.command_suggestion ? (
                          <ConfidenceBadge confidence={s.command_suggestion.confidence} />
                        ) : (
                          <Badge variant="outline" className="text-xs">none</Badge>
                        )}
                      </TableCell>
                    </TableRow>

                    {/* Expanded details */}
                    {expandedService === i && enabledServiceIndices.has(i) && (
                      <TableRow key={`detail-${i}`}>
                        <TableCell colSpan={7} className="bg-accent/30 p-4">
                          <div className="space-y-4">
                            {/* Commands */}
                            <div>
                              <h5 className="text-sm font-semibold mb-2 flex items-center gap-1">
                                <Terminal className="h-3.5 w-3.5" /> Commands
                                {s.command_suggestion && (
                                  <span className="font-normal text-xs text-muted-foreground ml-2">
                                    (source: {s.command_suggestion.source})
                                  </span>
                                )}
                              </h5>
                              <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
                                <div>
                                  <label className="text-xs text-muted-foreground">Check</label>
                                  <input
                                    type="text"
                                    value={s.command_suggestion?.check_cmd || ''}
                                    onChange={(e) => updateServiceCommand(i, 'check_cmd', e.target.value)}
                                    placeholder="e.g. systemctl is-active myservice"
                                    className="w-full rounded border border-input bg-background px-2 py-1 text-xs font-mono"
                                  />
                                </div>
                                <div>
                                  <label className="text-xs text-muted-foreground">Start</label>
                                  <input
                                    type="text"
                                    value={s.command_suggestion?.start_cmd || ''}
                                    onChange={(e) => updateServiceCommand(i, 'start_cmd', e.target.value)}
                                    placeholder="e.g. systemctl start myservice"
                                    className="w-full rounded border border-input bg-background px-2 py-1 text-xs font-mono"
                                  />
                                </div>
                                <div>
                                  <label className="text-xs text-muted-foreground">Stop</label>
                                  <input
                                    type="text"
                                    value={s.command_suggestion?.stop_cmd || ''}
                                    onChange={(e) => updateServiceCommand(i, 'stop_cmd', e.target.value)}
                                    placeholder="e.g. systemctl stop myservice"
                                    className="w-full rounded border border-input bg-background px-2 py-1 text-xs font-mono"
                                  />
                                </div>
                                <div>
                                  <label className="text-xs text-muted-foreground">Restart</label>
                                  <input
                                    type="text"
                                    value={s.command_suggestion?.restart_cmd || ''}
                                    onChange={(e) => updateServiceCommand(i, 'restart_cmd', e.target.value)}
                                    placeholder="e.g. systemctl restart myservice"
                                    className="w-full rounded border border-input bg-background px-2 py-1 text-xs font-mono"
                                  />
                                </div>
                              </div>
                            </div>

                            {/* Config files */}
                            {s.config_files && s.config_files.length > 0 && (
                              <div>
                                <h5 className="text-sm font-semibold mb-1 flex items-center gap-1">
                                  <FileText className="h-3.5 w-3.5" /> Config Files ({s.config_files.length})
                                </h5>
                                <div className="space-y-1">
                                  {s.config_files.map((cf, ci) => (
                                    <div key={ci} className="text-xs">
                                      <code className="font-mono text-muted-foreground">{cf.path}</code>
                                      {cf.extracted_endpoints && cf.extracted_endpoints.length > 0 && (
                                        <div className="ml-4 mt-1 space-y-0.5">
                                          {cf.extracted_endpoints.map((ep, ei) => (
                                            <div key={ei} className="flex items-center gap-2">
                                              <span className="text-xs font-medium">{ep.key}:</span>
                                              <code className="text-xs text-muted-foreground">{ep.value}</code>
                                              {ep.technology && (
                                                <Badge variant="secondary" className="text-[10px]">{ep.technology}</Badge>
                                              )}
                                            </div>
                                          ))}
                                        </div>
                                      )}
                                    </div>
                                  ))}
                                </div>
                              </div>
                            )}

                            {/* Log files */}
                            {s.log_files && s.log_files.length > 0 && (
                              <div>
                                <h5 className="text-sm font-semibold mb-1 flex items-center gap-1">
                                  <ScrollText className="h-3.5 w-3.5" /> Log Files ({s.log_files.length})
                                </h5>
                                <div className="space-y-0.5">
                                  {s.log_files.map((lf, li) => (
                                    <div key={li} className="text-xs flex items-center gap-2">
                                      <code className="font-mono text-muted-foreground">{lf.path}</code>
                                      <span className="text-muted-foreground">({formatBytes(lf.size_bytes)})</span>
                                    </div>
                                  ))}
                                </div>
                              </div>
                            )}

                            {s.matched_service && (
                              <div className="text-xs text-muted-foreground">
                                Matched system service: <code className="font-mono">{s.matched_service}</code>
                              </div>
                            )}
                          </div>
                        </TableCell>
                      </TableRow>
                    )}
                  </>
                ))}
              </TableBody>
            </Table>
          </div>
        </CardContent>
      </Card>

      {/* Dependencies */}
      <Card>
        <CardContent className="p-6 space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="font-semibold text-lg">
              Inferred Dependencies ({enabledDepIndices.size}/{dependencies.length})
            </h3>
            <p className="text-xs text-muted-foreground">Uncheck to remove false positives</p>
          </div>
          {dependencies.length === 0 ? (
            <p className="text-sm text-muted-foreground py-4">
              No cross-host dependencies detected. This can happen when agents are on isolated networks
              or when connections target hosts outside the selected scope.
            </p>
          ) : (
            <div className="border rounded-md max-h-48 overflow-y-auto">
              <Table>
                <TableHeader><TableRow>
                  <TableHead className="w-10"></TableHead>
                  <TableHead>From</TableHead>
                  <TableHead></TableHead>
                  <TableHead>To</TableHead>
                  <TableHead>Via</TableHead>
                  <TableHead>Detail</TableHead>
                </TableRow></TableHeader>
                <TableBody>
                  {dependencies.map((d, i) => {
                    const from = d.from_service_index !== null ? editedServices[d.from_service_index] : null;
                    const to = editedServices[d.to_service_index];
                    return (
                      <TableRow key={i} className={!enabledDepIndices.has(i) ? 'opacity-40' : ''}>
                        <TableCell>
                          <input
                            type="checkbox"
                            checked={enabledDepIndices.has(i)}
                            onChange={() => toggleDep(i)}
                            className="rounded"
                          />
                        </TableCell>
                        <TableCell className="text-sm">{from?.suggested_name || d.from_process || '?'}</TableCell>
                        <TableCell><ArrowRight className="h-4 w-4 text-muted-foreground" /></TableCell>
                        <TableCell className="text-sm">{to?.suggested_name || '?'}</TableCell>
                        <TableCell>
                          <Badge
                            variant={d.inferred_via === 'config_file' ? 'running' : 'outline'}
                            className="text-xs"
                          >
                            {d.inferred_via}
                          </Badge>
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground">
                          {d.config_key && <span>{d.config_key}</span>}
                          {d.technology && <Badge variant="secondary" className="text-[10px] ml-1">{d.technology}</Badge>}
                          {!d.config_key && <span>{d.remote_addr}:{d.remote_port}</span>}
                        </TableCell>
                      </TableRow>
                    );
                  })}
                </TableBody>
              </Table>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Scheduled Jobs */}
      {scheduledJobs.length > 0 && (
        <Card>
          <CardContent className="p-6 space-y-3">
            <h3 className="font-semibold text-lg flex items-center gap-2">
              <Clock className="h-5 w-5" />
              Scheduled Jobs ({scheduledJobs.length})
            </h3>
            <p className="text-sm text-muted-foreground">
              Cron jobs, systemd timers, and scheduled tasks discovered on the scanned servers.
              These can be promoted to batch components in the application map.
            </p>
            <div className="border rounded-md max-h-40 overflow-y-auto">
              <Table>
                <TableHeader><TableRow>
                  <TableHead>Host</TableHead>
                  <TableHead>Name</TableHead>
                  <TableHead>Schedule</TableHead>
                  <TableHead>Command</TableHead>
                  <TableHead>Source</TableHead>
                  <TableHead>User</TableHead>
                </TableRow></TableHeader>
                <TableBody>
                  {scheduledJobs.map((j, i) => (
                    <TableRow key={i}>
                      <TableCell className="text-xs font-mono">{j.hostname || '-'}</TableCell>
                      <TableCell className="font-medium text-xs">{j.name}</TableCell>
                      <TableCell className="font-mono text-xs">{j.schedule}</TableCell>
                      <TableCell className="font-mono text-xs truncate max-w-[200px]" title={j.command}>{j.command}</TableCell>
                      <TableCell><Badge variant="outline" className="text-[10px]">{j.source}</Badge></TableCell>
                      <TableCell className="text-xs">{j.user}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Unresolved connections */}
      {unresolvedConns.length > 0 && (
        <Card>
          <CardContent className="p-6 space-y-3">
            <h3 className="font-semibold text-lg flex items-center gap-2">
              <AlertTriangle className="h-5 w-5 text-orange-500" />
              Unresolved Connections ({unresolvedConns.length})
            </h3>
            <p className="text-sm text-muted-foreground">
              These connections target hosts not in the selected agent scope.
              Add more agents to resolve them, or they may be external services (DNS, NTP, cloud APIs).
            </p>
            <div className="border rounded-md max-h-32 overflow-y-auto">
              <Table>
                <TableHeader><TableRow>
                  <TableHead>From</TableHead><TableHead>Process</TableHead><TableHead>Remote</TableHead>
                </TableRow></TableHeader>
                <TableBody>
                  {unresolvedConns.map((c, i) => (
                    <TableRow key={i}>
                      <TableCell className="text-sm">{c.from_hostname}</TableCell>
                      <TableCell className="text-sm">{c.from_process || '-'}</TableCell>
                      <TableCell className="font-mono text-xs">{c.remote_addr}:{c.remote_port}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Create draft */}
      <div className="flex items-center gap-4">
        <button
          onClick={handleCreateDraft}
          disabled={!appName.trim() || enabledServiceIndices.size === 0 || createDraft.isPending}
          className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
        >
          {createDraft.isPending ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Plus className="h-4 w-4" />
          )}
          Create Draft
        </button>
        {createDraft.isSuccess && (
          <span className="text-sm text-green-700">
            <CheckCircle className="h-4 w-4 inline mr-1" />
            Draft created with {createDraft.data.components_created} components, {createDraft.data.dependencies_created} dependencies
          </span>
        )}
        {createDraft.isError && (
          <span className="text-sm text-red-700">
            <AlertCircle className="h-4 w-4 inline mr-1" />
            {(createDraft.error as Error)?.message || 'Failed'}
          </span>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Step 5: Drafts — view, apply, or continue editing
// ---------------------------------------------------------------------------

function DraftsStep() {
  const [selectedDraftId, setSelectedDraftId] = useState<string>();
  const { data: drafts, isLoading } = useDiscoveryDrafts();
  const { data: draft } = useDiscoveryDraft(selectedDraftId);
  const applyDraft = useApplyDraft();

  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader><TableRow>
              <TableHead>Name</TableHead><TableHead>Status</TableHead><TableHead>Created</TableHead>
            </TableRow></TableHeader>
            <TableBody>
              {isLoading ? (
                <TableRow>
                  <TableCell colSpan={3} className="text-center py-8"><Loader2 className="h-5 w-5 animate-spin mx-auto" /></TableCell>
                </TableRow>
              ) : !drafts?.length ? (
                <TableRow>
                  <TableCell colSpan={3} className="text-center text-muted-foreground py-8">
                    No drafts yet. Complete steps 1-4 to create one.
                  </TableCell>
                </TableRow>
              ) : (
                drafts.map(d => (
                  <TableRow
                    key={d.id}
                    onClick={() => setSelectedDraftId(d.id)}
                    className={`cursor-pointer ${selectedDraftId === d.id ? 'bg-accent' : ''}`}
                  >
                    <TableCell className="font-medium">{d.name}</TableCell>
                    <TableCell><DraftStatusBadge status={d.status} /></TableCell>
                    <TableCell className="text-sm text-muted-foreground">{new Date(d.inferred_at).toLocaleString()}</TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {draft && (
        <Card>
          <CardContent className="p-4 space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <h3 className="font-semibold text-lg">{draft.name}</h3>
                <p className="text-sm text-muted-foreground">
                  {draft.components.length} components, {draft.dependencies.length} dependencies
                </p>
              </div>
              <DraftStatusBadge status={draft.status} />
            </div>

            <div>
              <h4 className="text-sm font-semibold mb-1">Components</h4>
              <div className="border rounded-md max-h-48 overflow-y-auto">
                <Table>
                  <TableHeader><TableRow>
                    <TableHead>Name</TableHead><TableHead>Host</TableHead><TableHead>Type</TableHead><TableHead>Check</TableHead>
                  </TableRow></TableHeader>
                  <TableBody>
                    {draft.components.map(c => (
                      <TableRow key={c.id}>
                        <TableCell className="font-medium">{c.name}</TableCell>
                        <TableCell className="font-mono text-xs">{c.host || '-'}</TableCell>
                        <TableCell><Badge variant="secondary">{c.component_type}</Badge></TableCell>
                        <TableCell>
                          {c.check_cmd ? (
                            <ConfidenceBadge confidence={c.command_confidence} />
                          ) : (
                            <span className="text-xs text-muted-foreground">-</span>
                          )}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            </div>

            {draft.dependencies.length > 0 && (
              <div>
                <h4 className="text-sm font-semibold mb-1">Dependencies</h4>
                <div className="border rounded-md max-h-32 overflow-y-auto">
                  <Table>
                    <TableHeader><TableRow>
                      <TableHead>From</TableHead><TableHead></TableHead><TableHead>To</TableHead><TableHead>Via</TableHead>
                    </TableRow></TableHeader>
                    <TableBody>
                      {draft.dependencies.map((d, i) => {
                        const from = draft.components.find(c => c.id === d.from_component);
                        const to = draft.components.find(c => c.id === d.to_component);
                        return (
                          <TableRow key={i}>
                            <TableCell>{from?.name || '?'}</TableCell>
                            <TableCell><ArrowRight className="h-4 w-4 text-muted-foreground" /></TableCell>
                            <TableCell>{to?.name || '?'}</TableCell>
                            <TableCell><Badge variant="outline" className="text-xs">{d.inferred_via}</Badge></TableCell>
                          </TableRow>
                        );
                      })}
                    </TableBody>
                  </Table>
                </div>
              </div>
            )}

            {draft.status === 'pending' && (
              <div className="pt-2 space-y-2">
                <button
                  onClick={() => applyDraft.mutate(draft.id)}
                  disabled={applyDraft.isPending}
                  className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                >
                  {applyDraft.isPending ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <CheckCircle className="h-4 w-4" />
                  )}
                  Apply — Create Application & Map
                </button>
                {applyDraft.isSuccess && (
                  <div className="rounded-md bg-green-50 border border-green-200 p-3 text-sm text-green-800">
                    <CheckCircle className="h-4 w-4 inline mr-2" />
                    Application "{draft.name}" created with operational commands in advisory mode. Go to Dashboard to view the map.
                  </div>
                )}
              </div>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function DraftStatusBadge({ status }: { status: string }) {
  switch (status) {
    case 'pending': return <Badge variant="degraded">Pending</Badge>;
    case 'applied': return <Badge variant="running">Applied</Badge>;
    case 'dismissed': return <Badge variant="stopped">Dismissed</Badge>;
    default: return <Badge variant="outline">{status}</Badge>;
  }
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}
