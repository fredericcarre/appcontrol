import { useState } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from '@/components/ui/table';
import {
  Search,
  Radar,
  Network,
  Play,
  CheckCircle,
  AlertCircle,
  Loader2,
  Server,
  ArrowRight,
} from 'lucide-react';
import {
  useDiscoveryReports,
  useDiscoveryReport,
  useDiscoveryDrafts,
  useDiscoveryDraft,
  useTriggerAllScans,
  useInferTopology,
  useApplyDraft,
} from '@/api/discovery';
import { useAgents, type Agent } from '@/api/reports';

// ---------------------------------------------------------------------------
// Main Page
// ---------------------------------------------------------------------------

export function DiscoveryPage() {
  const [selectedReportId, setSelectedReportId] = useState<string>();
  const [selectedDraftId, setSelectedDraftId] = useState<string>();
  const [inferName, setInferName] = useState('');
  const [selectedAgents, setSelectedAgents] = useState<string[]>([]);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold flex items-center gap-2">
            <Radar className="h-6 w-6" />
            Discovery
          </h1>
          <p className="text-muted-foreground mt-1">
            Scan agents to discover processes, ports, and connections. Infer application topologies automatically.
          </p>
        </div>
        <ScanAllButton />
      </div>

      <Tabs defaultValue="reports">
        <TabsList>
          <TabsTrigger value="reports">Scan Reports</TabsTrigger>
          <TabsTrigger value="infer">Infer Topology</TabsTrigger>
          <TabsTrigger value="drafts">Drafts</TabsTrigger>
        </TabsList>

        <TabsContent value="reports">
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <ReportsList
              selectedId={selectedReportId}
              onSelect={setSelectedReportId}
            />
            {selectedReportId && (
              <ReportDetail reportId={selectedReportId} />
            )}
          </div>
        </TabsContent>

        <TabsContent value="infer">
          <InferPanel
            name={inferName}
            setName={setInferName}
            selectedAgents={selectedAgents}
            setSelectedAgents={setSelectedAgents}
          />
        </TabsContent>

        <TabsContent value="drafts">
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <DraftsList
              selectedId={selectedDraftId}
              onSelect={setSelectedDraftId}
            />
            {selectedDraftId && (
              <DraftDetail draftId={selectedDraftId} />
            )}
          </div>
        </TabsContent>
      </Tabs>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Scan All Button
// ---------------------------------------------------------------------------

function ScanAllButton() {
  const triggerAll = useTriggerAllScans();

  return (
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
  );
}

// ---------------------------------------------------------------------------
// Reports List
// ---------------------------------------------------------------------------

function ReportsList({
  selectedId,
  onSelect,
}: {
  selectedId?: string;
  onSelect: (id: string) => void;
}) {
  const { data: reports, isLoading } = useDiscoveryReports();

  if (isLoading) {
    return (
      <Card>
        <CardContent className="flex items-center justify-center py-12">
          <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardContent className="p-0">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Hostname</TableHead>
              <TableHead>Agent</TableHead>
              <TableHead>Scanned</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {!reports?.length ? (
              <TableRow>
                <TableCell colSpan={3} className="text-center text-muted-foreground py-8">
                  No discovery reports yet. Click "Scan All Agents" to start.
                </TableCell>
              </TableRow>
            ) : (
              reports.map((r) => (
                <TableRow
                  key={r.id}
                  onClick={() => onSelect(r.id)}
                  className={`cursor-pointer ${selectedId === r.id ? 'bg-accent' : ''}`}
                >
                  <TableCell>
                    <div className="flex items-center gap-2">
                      <Server className="h-4 w-4 text-muted-foreground" />
                      <span className="font-medium">{r.hostname}</span>
                    </div>
                  </TableCell>
                  <TableCell className="font-mono text-xs text-muted-foreground">
                    {r.agent_id?.slice(0, 8)}
                  </TableCell>
                  <TableCell className="text-sm text-muted-foreground">
                    {new Date(r.scanned_at).toLocaleString()}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Report Detail
// ---------------------------------------------------------------------------

function ReportDetail({ reportId }: { reportId: string }) {
  const { data: report, isLoading } = useDiscoveryReport(reportId);

  if (isLoading || !report) {
    return (
      <Card>
        <CardContent className="flex items-center justify-center py-12">
          <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
        </CardContent>
      </Card>
    );
  }

  const { processes = [], listeners = [], connections = [], services = [] } = report.report;

  return (
    <Card>
      <CardContent className="p-4 space-y-4">
        <div className="flex items-center justify-between">
          <h3 className="font-semibold text-lg">{report.hostname}</h3>
          <span className="text-sm text-muted-foreground">
            {new Date(report.scanned_at).toLocaleString()}
          </span>
        </div>

        {/* Stats summary */}
        <div className="grid grid-cols-4 gap-3">
          <StatCard label="Processes" value={processes.length} icon={<Search className="h-4 w-4" />} />
          <StatCard label="Listeners" value={listeners.length} icon={<Network className="h-4 w-4" />} />
          <StatCard label="Connections" value={connections.length} icon={<ArrowRight className="h-4 w-4" />} />
          <StatCard label="Services" value={services.length} icon={<Server className="h-4 w-4" />} />
        </div>

        {/* Listeners table */}
        <div>
          <h4 className="text-sm font-semibold mb-2">TCP Listeners</h4>
          <div className="border rounded-md max-h-48 overflow-y-auto">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-20">Port</TableHead>
                  <TableHead>Process</TableHead>
                  <TableHead>PID</TableHead>
                  <TableHead>Address</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {listeners.map((l, i) => (
                  <TableRow key={i}>
                    <TableCell className="font-mono font-semibold">{l.port}</TableCell>
                    <TableCell>{l.process_name || '-'}</TableCell>
                    <TableCell className="font-mono text-xs">{l.pid || '-'}</TableCell>
                    <TableCell className="text-muted-foreground">{l.address}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </div>

        {/* Connections table */}
        {connections.length > 0 && (
          <div>
            <h4 className="text-sm font-semibold mb-2">Outbound Connections</h4>
            <div className="border rounded-md max-h-48 overflow-y-auto">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Process</TableHead>
                    <TableHead>Remote</TableHead>
                    <TableHead>Local Port</TableHead>
                  </TableRow>
                </TableHeader>
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

        {/* Processes with listening ports */}
        {processes.filter(p => p.listening_ports.length > 0).length > 0 && (
          <div>
            <h4 className="text-sm font-semibold mb-2">Application Processes (with ports)</h4>
            <div className="border rounded-md max-h-48 overflow-y-auto">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Process</TableHead>
                    <TableHead>PID</TableHead>
                    <TableHead>Ports</TableHead>
                    <TableHead>User</TableHead>
                    <TableHead>Memory</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {processes.filter(p => p.listening_ports.length > 0).map((p) => (
                    <TableRow key={p.pid}>
                      <TableCell className="font-medium">{p.name}</TableCell>
                      <TableCell className="font-mono text-xs">{p.pid}</TableCell>
                      <TableCell>
                        <div className="flex gap-1 flex-wrap">
                          {p.listening_ports.map(port => (
                            <Badge key={port} variant="secondary" className="text-xs font-mono">{port}</Badge>
                          ))}
                        </div>
                      </TableCell>
                      <TableCell className="text-xs">{p.user}</TableCell>
                      <TableCell className="text-xs">{formatBytes(p.memory_bytes)}</TableCell>
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
// Infer Panel
// ---------------------------------------------------------------------------

function InferPanel({
  name,
  setName,
  selectedAgents,
  setSelectedAgents,
}: {
  name: string;
  setName: (v: string) => void;
  selectedAgents: string[];
  setSelectedAgents: (v: string[]) => void;
}) {
  const { data: agentsData } = useAgents();
  const { data: reports } = useDiscoveryReports();
  const infer = useInferTopology();

  // Extract agent list — handle both array and {agents: []} shapes
  const agents: Agent[] = Array.isArray(agentsData)
    ? agentsData
    : (agentsData as unknown as { agents?: Agent[] })?.agents || [];

  // Only show agents that have at least one report
  const agentIdsWithReports = new Set(reports?.map(r => r.agent_id) || []);
  const eligibleAgents = agents.filter(a => agentIdsWithReports.has(a.id));

  const toggleAgent = (id: string) => {
    setSelectedAgents(
      selectedAgents.includes(id)
        ? selectedAgents.filter(a => a !== id)
        : [...selectedAgents, id]
    );
  };

  const canInfer = name.trim().length > 0 && selectedAgents.length > 0 && !infer.isPending;

  return (
    <div className="space-y-4">
      <Card>
        <CardContent className="p-6 space-y-4">
          <h3 className="font-semibold text-lg flex items-center gap-2">
            <Network className="h-5 w-5" />
            Infer Application Topology
          </h3>
          <p className="text-sm text-muted-foreground">
            Select agents with scan data and give the application a name.
            The inference engine will correlate listeners and connections across agents
            to build a dependency graph.
          </p>

          <div>
            <label className="block text-sm font-medium mb-1">Application Name</label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. order-processing-stack"
              className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
            />
          </div>

          <div>
            <label className="block text-sm font-medium mb-2">Select Agents</label>
            {eligibleAgents.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                No agents have scan reports. Run "Scan All Agents" first.
              </p>
            ) : (
              <div className="space-y-2">
                {eligibleAgents.map(agent => (
                  <label
                    key={agent.id}
                    className={`flex items-center gap-3 p-3 rounded-md border cursor-pointer transition-colors ${
                      selectedAgents.includes(agent.id)
                        ? 'border-primary bg-primary/5'
                        : 'border-border hover:bg-accent'
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
                    <span className="text-xs text-muted-foreground ml-auto">
                      {agent.status === 'connected' ? (
                        <Badge variant="running" className="text-xs">Connected</Badge>
                      ) : (
                        <Badge variant="stopped" className="text-xs">Offline</Badge>
                      )}
                    </span>
                  </label>
                ))}
              </div>
            )}
          </div>

          <button
            onClick={() => infer.mutate({ name, agent_ids: selectedAgents })}
            disabled={!canInfer}
            className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            {infer.isPending ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Play className="h-4 w-4" />
            )}
            Run Inference
          </button>

          {infer.isSuccess && (
            <div className="rounded-md bg-green-50 border border-green-200 p-3 text-sm text-green-800">
              <CheckCircle className="h-4 w-4 inline mr-2" />
              Draft created: {infer.data.components_inferred} components,{' '}
              {infer.data.dependencies_inferred} dependencies inferred.
              Check the Drafts tab.
            </div>
          )}

          {infer.isError && (
            <div className="rounded-md bg-red-50 border border-red-200 p-3 text-sm text-red-800">
              <AlertCircle className="h-4 w-4 inline mr-2" />
              {(infer.error as Error)?.message || 'Inference failed'}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Drafts List
// ---------------------------------------------------------------------------

function DraftsList({
  selectedId,
  onSelect,
}: {
  selectedId?: string;
  onSelect: (id: string) => void;
}) {
  const { data: drafts, isLoading } = useDiscoveryDrafts();

  if (isLoading) {
    return (
      <Card>
        <CardContent className="flex items-center justify-center py-12">
          <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardContent className="p-0">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Inferred</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {!drafts?.length ? (
              <TableRow>
                <TableCell colSpan={3} className="text-center text-muted-foreground py-8">
                  No drafts yet. Use the "Infer Topology" tab to create one.
                </TableCell>
              </TableRow>
            ) : (
              drafts.map((d) => (
                <TableRow
                  key={d.id}
                  onClick={() => onSelect(d.id)}
                  className={`cursor-pointer ${selectedId === d.id ? 'bg-accent' : ''}`}
                >
                  <TableCell className="font-medium">{d.name}</TableCell>
                  <TableCell>
                    <DraftStatusBadge status={d.status} />
                  </TableCell>
                  <TableCell className="text-sm text-muted-foreground">
                    {new Date(d.inferred_at).toLocaleString()}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Draft Detail
// ---------------------------------------------------------------------------

function DraftDetail({ draftId }: { draftId: string }) {
  const { data: draft, isLoading } = useDiscoveryDraft(draftId);
  const applyDraft = useApplyDraft();

  if (isLoading || !draft) {
    return (
      <Card>
        <CardContent className="flex items-center justify-center py-12">
          <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
        </CardContent>
      </Card>
    );
  }

  return (
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

        {/* Components */}
        <div>
          <h4 className="text-sm font-semibold mb-2">Components</h4>
          <div className="border rounded-md max-h-48 overflow-y-auto">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Process</TableHead>
                  <TableHead>Host</TableHead>
                  <TableHead>Type</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {draft.components.map((c) => (
                  <TableRow key={c.id}>
                    <TableCell className="font-medium">{c.name}</TableCell>
                    <TableCell className="text-muted-foreground">{c.process_name || '-'}</TableCell>
                    <TableCell className="font-mono text-xs">{c.host || '-'}</TableCell>
                    <TableCell>
                      <Badge variant="secondary">{c.component_type}</Badge>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </div>

        {/* Dependencies */}
        {draft.dependencies.length > 0 && (
          <div>
            <h4 className="text-sm font-semibold mb-2">Inferred Dependencies</h4>
            <div className="border rounded-md max-h-48 overflow-y-auto">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>From</TableHead>
                    <TableHead></TableHead>
                    <TableHead>To</TableHead>
                    <TableHead>Via</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {draft.dependencies.map((d, i) => {
                    const from = draft.components.find(c => c.id === d.from_component);
                    const to = draft.components.find(c => c.id === d.to_component);
                    return (
                      <TableRow key={i}>
                        <TableCell className="font-medium">{from?.name || d.from_component.slice(0, 8)}</TableCell>
                        <TableCell><ArrowRight className="h-4 w-4 text-muted-foreground" /></TableCell>
                        <TableCell className="font-medium">{to?.name || d.to_component.slice(0, 8)}</TableCell>
                        <TableCell>
                          <Badge variant="outline" className="text-xs">{d.inferred_via}</Badge>
                        </TableCell>
                      </TableRow>
                    );
                  })}
                </TableBody>
              </Table>
            </div>
          </div>
        )}

        {/* Apply button */}
        {draft.status === 'pending' && (
          <div className="pt-2">
            <button
              onClick={() => applyDraft.mutate(draftId)}
              disabled={applyDraft.isPending}
              className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              {applyDraft.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <CheckCircle className="h-4 w-4" />
              )}
              Apply Draft — Create Application
            </button>

            {applyDraft.isSuccess && (
              <div className="mt-2 rounded-md bg-green-50 border border-green-200 p-3 text-sm text-green-800">
                <CheckCircle className="h-4 w-4 inline mr-2" />
                Application created! Go to Dashboard to view it.
              </div>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function StatCard({ label, value, icon }: { label: string; value: number; icon: React.ReactNode }) {
  return (
    <div className="border rounded-md p-3 text-center">
      <div className="flex items-center justify-center gap-1 text-muted-foreground mb-1">{icon}</div>
      <div className="text-xl font-bold">{value}</div>
      <div className="text-xs text-muted-foreground">{label}</div>
    </div>
  );
}

function DraftStatusBadge({ status }: { status: string }) {
  switch (status) {
    case 'pending':
      return <Badge variant="degraded">Pending</Badge>;
    case 'applied':
      return <Badge variant="running">Applied</Badge>;
    case 'dismissed':
      return <Badge variant="stopped">Dismissed</Badge>;
    default:
      return <Badge variant="outline">{status}</Badge>;
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}
