import { useState, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Radar,
  CheckCircle,
  Loader2,
  Server,
  ArrowRight,
  Network,
  Rocket,
  RotateCcw,
  X,
  AlertTriangle,
  Calendar,
  FileJson,
  GitCompare,
  MoreVertical,
  Plus,
  Trash2,
  Clock,
  Eye,
} from 'lucide-react';
import {
  useDiscoveryReports,
  useDiscoveryReport,
  useTriggerAllScans,
  useCorrelate,
  useSnapshotSchedules,
  useScheduledSnapshots,
  useCreateSchedule,
  useDeleteSchedule,
  useCompareSnapshots,
  type SnapshotSchedule,
  type ScheduledSnapshot,
  type DiscoveryReportDetail,
} from '@/api/discovery';
import { useAgents, type Agent } from '@/api/reports';
import { useDiscoveryStore } from '@/stores/discovery';
import { TopologyMap } from '@/components/discovery/TopologyMap';
import { LayerSidebar } from '@/components/discovery/LayerSidebar';
import { ServiceDetailPanel } from '@/components/discovery/ServiceDetailPanel';
import { TopologyToolbar } from '@/components/discovery/TopologyToolbar';
import { DiscoveryStepper } from '@/components/discovery/DiscoveryStepper';
import { AgentManagementPanel } from '@/components/discovery/AgentManagementPanel';
import { MatrixRain } from '@/components/discovery/MatrixRain';
import { classifyConfidence } from '@/components/discovery/confidence';
// StagingArea removed - services are in the sidebar

// ---------------------------------------------------------------------------
// Main Page — 3-phase flow (Scan → Map → Done) with additional views
// ---------------------------------------------------------------------------

type DiscoveryView = 'scan' | 'schedules' | 'snapshots' | 'raw';

export function DiscoveryPage() {
  const phase = useDiscoveryStore((s) => s.phase);
  const [view, setView] = useState<DiscoveryView>('scan');

  // When in map or done phase, always show that phase
  if (phase === 'map') {
    return (
      <div className="flex flex-col h-full">
        <MapPhase />
      </div>
    );
  }

  if (phase === 'done') {
    return (
      <div className="flex flex-col h-full">
        <div className="mb-6 py-4 border-b border-border">
          <DiscoveryStepper currentPhase={phase} />
        </div>
        <DonePhase />
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header with view tabs */}
      <div className="mb-6 py-4 border-b border-border">
        <div className="flex items-center justify-between mb-4">
          <h1 className="text-2xl font-bold flex items-center gap-2">
            <Radar className="h-6 w-6" />
            Discovery
          </h1>
        </div>
        <Tabs value={view} onValueChange={(v) => setView(v as DiscoveryView)}>
          <TabsList>
            <TabsTrigger value="scan" className="gap-2">
              <Radar className="h-4 w-4" />
              Scan & Analyze
            </TabsTrigger>
            <TabsTrigger value="schedules" className="gap-2">
              <Calendar className="h-4 w-4" />
              Schedules
            </TabsTrigger>
            <TabsTrigger value="snapshots" className="gap-2">
              <GitCompare className="h-4 w-4" />
              Snapshots & Diffs
            </TabsTrigger>
            <TabsTrigger value="raw" className="gap-2">
              <FileJson className="h-4 w-4" />
              Raw Data
            </TabsTrigger>
          </TabsList>
        </Tabs>
      </div>

      {/* View content */}
      {view === 'scan' && <ScanPhase />}
      {view === 'schedules' && <SchedulesView />}
      {view === 'snapshots' && <SnapshotsView />}
      {view === 'raw' && <RawDataView />}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Schedules View — Manage scheduled collections
// ---------------------------------------------------------------------------

function SchedulesView() {
  const { data: schedules, isLoading } = useSnapshotSchedules();
  const { data: agentsData } = useAgents();
  const createSchedule = useCreateSchedule();
  const deleteSchedule = useDeleteSchedule();

  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [newScheduleName, setNewScheduleName] = useState('');
  const [newScheduleFrequency, setNewScheduleFrequency] = useState<'hourly' | 'daily' | 'weekly' | 'monthly'>('daily');
  const [selectedAgentIds, setSelectedAgentIds] = useState<string[]>([]);

  const agents: Agent[] = useMemo(() => {
    return Array.isArray(agentsData)
      ? agentsData
      : (agentsData as unknown as { agents?: Agent[] })?.agents || [];
  }, [agentsData]);

  const handleCreate = async () => {
    if (!newScheduleName.trim() || selectedAgentIds.length === 0) return;
    await createSchedule.mutateAsync({
      name: newScheduleName.trim(),
      agent_ids: selectedAgentIds,
      frequency: newScheduleFrequency,
    });
    setCreateDialogOpen(false);
    setNewScheduleName('');
    setSelectedAgentIds([]);
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <p className="text-muted-foreground">
          Schedule automatic discovery scans to track infrastructure changes over time.
        </p>
        <Button onClick={() => setCreateDialogOpen(true)} className="gap-2">
          <Plus className="h-4 w-4" />
          New Schedule
        </Button>
      </div>

      {schedules && schedules.length > 0 ? (
        <div className="grid gap-4">
          {schedules.map((schedule) => (
            <Card key={schedule.id}>
              <CardContent className="p-4">
                <div className="flex items-center justify-between">
                  <div className="space-y-1">
                    <div className="flex items-center gap-2">
                      <Calendar className="h-4 w-4 text-muted-foreground" />
                      <span className="font-medium">{schedule.name}</span>
                      <Badge variant={schedule.enabled ? 'default' : 'secondary'}>
                        {schedule.enabled ? 'Active' : 'Paused'}
                      </Badge>
                    </div>
                    <div className="text-sm text-muted-foreground flex items-center gap-4">
                      <span className="flex items-center gap-1">
                        <Clock className="h-3 w-3" />
                        {schedule.frequency}
                      </span>
                      <span>{schedule.agent_ids.length} agent(s)</span>
                      {schedule.next_run_at && (
                        <span>Next: {new Date(schedule.next_run_at).toLocaleString()}</span>
                      )}
                    </div>
                  </div>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => deleteSchedule.mutate(schedule.id)}
                    className="text-destructive hover:text-destructive"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      ) : (
        <Card>
          <CardContent className="p-8 text-center">
            <Calendar className="h-12 w-12 mx-auto text-muted-foreground mb-4" />
            <p className="text-muted-foreground">No schedules configured yet.</p>
            <p className="text-sm text-muted-foreground mt-1">
              Create a schedule to automatically collect discovery data.
            </p>
          </CardContent>
        </Card>
      )}

      {/* Create Schedule Dialog */}
      <Dialog open={createDialogOpen} onOpenChange={setCreateDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Create Schedule</DialogTitle>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label>Name</Label>
              <Input
                value={newScheduleName}
                onChange={(e) => setNewScheduleName(e.target.value)}
                placeholder="e.g. Daily Production Scan"
              />
            </div>
            <div className="space-y-2">
              <Label>Frequency</Label>
              <Select value={newScheduleFrequency} onValueChange={(v) => setNewScheduleFrequency(v as typeof newScheduleFrequency)}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="hourly">Hourly</SelectItem>
                  <SelectItem value="daily">Daily</SelectItem>
                  <SelectItem value="weekly">Weekly</SelectItem>
                  <SelectItem value="monthly">Monthly</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label>Agents ({selectedAgentIds.length} selected)</Label>
              <div className="max-h-48 overflow-y-auto border rounded-md p-2 space-y-1">
                {agents.map((agent) => (
                  <label key={agent.id} className="flex items-center gap-2 p-1 hover:bg-accent rounded cursor-pointer">
                    <input
                      type="checkbox"
                      checked={selectedAgentIds.includes(agent.id)}
                      onChange={(e) => {
                        if (e.target.checked) {
                          setSelectedAgentIds([...selectedAgentIds, agent.id]);
                        } else {
                          setSelectedAgentIds(selectedAgentIds.filter((id) => id !== agent.id));
                        }
                      }}
                      className="h-4 w-4"
                    />
                    <span className="text-sm">{agent.hostname || agent.id}</span>
                  </label>
                ))}
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateDialogOpen(false)}>Cancel</Button>
            <Button onClick={handleCreate} disabled={!newScheduleName.trim() || selectedAgentIds.length === 0}>
              Create
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Snapshots View — View and compare snapshots
// ---------------------------------------------------------------------------

function SnapshotsView() {
  const { data: snapshots, isLoading } = useScheduledSnapshots();
  const compareSnapshots = useCompareSnapshots();

  const [snapshot1, setSnapshot1] = useState<string | null>(null);
  const [snapshot2, setSnapshot2] = useState<string | null>(null);

  const handleCompare = async () => {
    if (!snapshot1 || !snapshot2) return;
    await compareSnapshots.mutateAsync({
      snapshot_id_1: snapshot1,
      snapshot_id_2: snapshot2,
    });
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <p className="text-muted-foreground">
        Compare snapshots to see what changed in your infrastructure.
      </p>

      {/* Comparison selector */}
      <Card>
        <CardContent className="p-4">
          <div className="flex items-center gap-4">
            <div className="flex-1 space-y-2">
              <Label>Snapshot 1 (older)</Label>
              <Select value={snapshot1 || ''} onValueChange={setSnapshot1}>
                <SelectTrigger>
                  <SelectValue placeholder="Select snapshot...">
                    {snapshot1 ? (
                      <span>
                        {snapshots?.find(s => s.id === snapshot1)?.schedule_name} -{' '}
                        {snapshots?.find(s => s.id === snapshot1)?.captured_at
                          ? new Date(snapshots.find(s => s.id === snapshot1)!.captured_at).toLocaleString()
                          : ''}
                      </span>
                    ) : (
                      <span className="text-muted-foreground">Select snapshot...</span>
                    )}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {snapshots?.map((s) => (
                    <SelectItem key={s.id} value={s.id}>
                      {s.schedule_name} - {new Date(s.captured_at).toLocaleString()}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <GitCompare className="h-5 w-5 text-muted-foreground mt-6" />
            <div className="flex-1 space-y-2">
              <Label>Snapshot 2 (newer)</Label>
              <Select value={snapshot2 || ''} onValueChange={setSnapshot2}>
                <SelectTrigger>
                  <SelectValue placeholder="Select snapshot...">
                    {snapshot2 ? (
                      <span>
                        {snapshots?.find(s => s.id === snapshot2)?.schedule_name} -{' '}
                        {snapshots?.find(s => s.id === snapshot2)?.captured_at
                          ? new Date(snapshots.find(s => s.id === snapshot2)!.captured_at).toLocaleString()
                          : ''}
                      </span>
                    ) : (
                      <span className="text-muted-foreground">Select snapshot...</span>
                    )}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {snapshots?.map((s) => (
                    <SelectItem key={s.id} value={s.id}>
                      {s.schedule_name} - {new Date(s.captured_at).toLocaleString()}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <Button
              onClick={handleCompare}
              disabled={!snapshot1 || !snapshot2 || compareSnapshots.isPending}
              className="mt-6"
            >
              {compareSnapshots.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                'Compare'
              )}
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Comparison results */}
      {compareSnapshots.data && (
        <div className="space-y-4">
          <h3 className="font-semibold">Comparison Results</h3>
          <div className="grid grid-cols-3 gap-4">
            <Card>
              <CardContent className="p-4">
                <div className="text-2xl font-bold text-green-600">{compareSnapshots.data.added.length}</div>
                <div className="text-sm text-muted-foreground">Added Services</div>
              </CardContent>
            </Card>
            <Card>
              <CardContent className="p-4">
                <div className="text-2xl font-bold text-red-600">{compareSnapshots.data.removed.length}</div>
                <div className="text-sm text-muted-foreground">Removed Services</div>
              </CardContent>
            </Card>
            <Card>
              <CardContent className="p-4">
                <div className="text-2xl font-bold text-amber-600">{compareSnapshots.data.modified.length}</div>
                <div className="text-sm text-muted-foreground">Modified Services</div>
              </CardContent>
            </Card>
          </div>

          {/* Details of changes */}
          {compareSnapshots.data.added.length > 0 && (
            <Card>
              <CardContent className="p-4">
                <h4 className="font-medium mb-2 text-green-600">Added Services</h4>
                <div className="space-y-1">
                  {compareSnapshots.data.added.map((svc, i) => (
                    <div key={i} className="text-sm">
                      {svc.suggested_name} ({svc.hostname}:{svc.ports.join(', ')})
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}

          {compareSnapshots.data.removed.length > 0 && (
            <Card>
              <CardContent className="p-4">
                <h4 className="font-medium mb-2 text-red-600">Removed Services</h4>
                <div className="space-y-1">
                  {compareSnapshots.data.removed.map((svc, i) => (
                    <div key={i} className="text-sm">
                      {svc.suggested_name} ({svc.hostname}:{svc.ports.join(', ')})
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}
        </div>
      )}

      {/* List of snapshots */}
      {snapshots && snapshots.length > 0 ? (
        <div className="space-y-2">
          <h3 className="font-semibold">All Snapshots</h3>
          <div className="grid gap-2">
            {snapshots.map((snapshot) => (
              <Card key={snapshot.id}>
                <CardContent className="p-3 flex items-center justify-between">
                  <div>
                    <span className="font-medium">{snapshot.schedule_name}</span>
                    <span className="text-sm text-muted-foreground ml-2">
                      {new Date(snapshot.captured_at).toLocaleString()}
                    </span>
                  </div>
                  <Badge variant="secondary">{snapshot.report_ids.length} reports</Badge>
                </CardContent>
              </Card>
            ))}
          </div>
        </div>
      ) : (
        <Card>
          <CardContent className="p-8 text-center">
            <GitCompare className="h-12 w-12 mx-auto text-muted-foreground mb-4" />
            <p className="text-muted-foreground">No snapshots available yet.</p>
            <p className="text-sm text-muted-foreground mt-1">
              Snapshots are created by scheduled scans.
            </p>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Raw Data View — View raw discovery reports
// ---------------------------------------------------------------------------

function RawDataView() {
  const { data: reports, isLoading } = useDiscoveryReports();
  const [selectedReportId, setSelectedReportId] = useState<string | null>(null);
  const { data: reportDetail } = useDiscoveryReport(selectedReportId || undefined);

  // Deduplicate reports by id (in case API returns duplicates)
  const uniqueReports = useMemo(() => {
    if (!reports) return [];
    const seen = new Set<string>();
    return reports.filter((r) => {
      if (seen.has(r.id)) return false;
      seen.add(r.id);
      return true;
    });
  }, [reports]);

  // Collect unique users from processes
  const usersFromProcesses = useMemo(() => {
    if (!reportDetail?.report?.processes) return [];
    const users = new Set<string>();
    reportDetail.report.processes.forEach((p) => {
      if (p.user) users.add(p.user);
    });
    return Array.from(users).sort();
  }, [reportDetail]);

  // Collect log files from all processes
  const allLogFiles = useMemo(() => {
    if (!reportDetail?.report?.processes) return [];
    const logs: Array<{ path: string; size_bytes: number; process: string }> = [];
    reportDetail.report.processes.forEach((p) => {
      p.log_files?.forEach((lf) => {
        logs.push({ ...lf, process: p.name });
      });
    });
    return logs;
  }, [reportDetail]);

  // Collect config files from all processes
  const allConfigFiles = useMemo(() => {
    if (!reportDetail?.report?.processes) return [];
    const configs: Array<{ path: string; process: string; endpoints: number }> = [];
    reportDetail.report.processes.forEach((p) => {
      p.config_files?.forEach((cf) => {
        configs.push({
          path: cf.path,
          process: p.name,
          endpoints: cf.extracted_endpoints?.length || 0,
        });
      });
    });
    return configs;
  }, [reportDetail]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <p className="text-muted-foreground">
        View raw discovery data collected from agents.
      </p>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Reports list */}
        <Card>
          <CardContent className="p-4">
            <h3 className="font-semibold mb-3">Discovery Reports</h3>
            {uniqueReports.length > 0 ? (
              <div className="space-y-1 max-h-[600px] overflow-y-auto">
                {uniqueReports.map((report) => (
                  <button
                    key={report.id}
                    onClick={() => setSelectedReportId(report.id)}
                    className={`w-full text-left p-2 rounded-md hover:bg-accent transition-colors ${
                      selectedReportId === report.id ? 'bg-accent' : ''
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="font-medium text-sm">{report.hostname}</span>
                      <Eye className="h-3 w-3 text-muted-foreground" />
                    </div>
                    <div className="text-xs text-muted-foreground">
                      {new Date(report.scanned_at).toLocaleString()}
                    </div>
                  </button>
                ))}
              </div>
            ) : (
              <p className="text-sm text-muted-foreground text-center py-4">
                No reports available. Run a discovery scan first.
              </p>
            )}
          </CardContent>
        </Card>

        {/* Report detail - spans 2 columns */}
        <Card className="lg:col-span-2">
          <CardContent className="p-4">
            <h3 className="font-semibold mb-3">Report Detail</h3>
            {reportDetail ? (
              <div className="space-y-4">
                <div className="flex items-center gap-2 text-sm">
                  <Server className="h-4 w-4 text-muted-foreground" />
                  <span className="font-medium">{reportDetail.hostname}</span>
                  <span className="text-muted-foreground">
                    - {new Date(reportDetail.scanned_at).toLocaleString()}
                  </span>
                </div>

                {/* Summary badges */}
                <div className="flex flex-wrap gap-2">
                  <Badge variant="secondary">
                    {reportDetail.report?.processes?.length || 0} processes
                  </Badge>
                  <Badge variant="secondary">
                    {reportDetail.report?.services?.length || 0} services
                  </Badge>
                  <Badge variant="secondary">
                    {reportDetail.report?.listeners?.length || 0} listeners
                  </Badge>
                  <Badge variant="secondary">
                    {reportDetail.report?.scheduled_jobs?.length || 0} cron jobs
                  </Badge>
                  <Badge variant="secondary">
                    {usersFromProcesses.length} users
                  </Badge>
                  <Badge variant="secondary">
                    {allLogFiles.length} log files
                  </Badge>
                </div>

                <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
                  {/* Processes */}
                  {reportDetail.report?.processes && (
                    <div>
                      <h4 className="text-sm font-medium mb-2">
                        Processes ({reportDetail.report.processes.length})
                      </h4>
                      <div className="max-h-40 overflow-y-auto text-xs space-y-1 border rounded p-2">
                        {reportDetail.report.processes
                          .filter((p) => p.listening_ports.length > 0)
                          .slice(0, 30)
                          .map((p, i) => (
                            <div key={i} className="p-1 bg-muted rounded flex items-center justify-between">
                              <div>
                                <span className="font-mono font-medium">{p.name}</span>
                                <span className="text-muted-foreground ml-1">({p.user})</span>
                              </div>
                              <span className="text-blue-600 font-mono">
                                :{p.listening_ports.join(', ')}
                              </span>
                            </div>
                          ))}
                        {reportDetail.report.processes.filter((p) => p.listening_ports.length > 0).length > 30 && (
                          <div className="text-muted-foreground text-center">
                            ... and more
                          </div>
                        )}
                      </div>
                    </div>
                  )}

                  {/* System Services */}
                  {reportDetail.report?.services && reportDetail.report.services.length > 0 && (
                    <div>
                      <h4 className="text-sm font-medium mb-2">
                        System Services ({reportDetail.report.services.length})
                      </h4>
                      <div className="max-h-40 overflow-y-auto text-xs space-y-1 border rounded p-2">
                        {reportDetail.report.services.slice(0, 20).map((s, i) => (
                          <div key={i} className="p-1 bg-muted rounded flex items-center justify-between">
                            <span className="font-mono">{s.name}</span>
                            <Badge
                              variant={s.status === 'running' ? 'default' : 'secondary'}
                              className="text-[10px]"
                            >
                              {s.status}
                            </Badge>
                          </div>
                        ))}
                        {reportDetail.report.services.length > 20 && (
                          <div className="text-muted-foreground text-center">
                            ... and {reportDetail.report.services.length - 20} more
                          </div>
                        )}
                      </div>
                    </div>
                  )}

                  {/* Scheduled Jobs (Cron) */}
                  {reportDetail.report?.scheduled_jobs && reportDetail.report.scheduled_jobs.length > 0 && (
                    <div>
                      <h4 className="text-sm font-medium mb-2">
                        Scheduled Jobs ({reportDetail.report.scheduled_jobs.length})
                      </h4>
                      <div className="max-h-40 overflow-y-auto text-xs space-y-1 border rounded p-2">
                        {reportDetail.report.scheduled_jobs.map((job, i) => (
                          <div key={i} className="p-1 bg-muted rounded">
                            <div className="flex items-center justify-between">
                              <span className="font-medium">{job.name}</span>
                              <span className="text-muted-foreground">{job.user}</span>
                            </div>
                            <div className="text-muted-foreground font-mono text-[10px] truncate">
                              {job.schedule} • {job.command}
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Users */}
                  {usersFromProcesses.length > 0 && (
                    <div>
                      <h4 className="text-sm font-medium mb-2">
                        Users Running Processes ({usersFromProcesses.length})
                      </h4>
                      <div className="max-h-40 overflow-y-auto text-xs border rounded p-2">
                        <div className="flex flex-wrap gap-1">
                          {usersFromProcesses.map((user, i) => (
                            <Badge key={i} variant="outline" className="text-[10px]">
                              {user}
                            </Badge>
                          ))}
                        </div>
                      </div>
                    </div>
                  )}

                  {/* Log Files */}
                  {allLogFiles.length > 0 && (
                    <div>
                      <h4 className="text-sm font-medium mb-2">
                        Log Files ({allLogFiles.length})
                      </h4>
                      <div className="max-h-40 overflow-y-auto text-xs space-y-1 border rounded p-2">
                        {allLogFiles.slice(0, 20).map((lf, i) => (
                          <div key={i} className="p-1 bg-muted rounded flex items-center justify-between">
                            <span className="font-mono truncate flex-1" title={lf.path}>
                              {lf.path.split('/').pop()}
                            </span>
                            <span className="text-muted-foreground ml-2">
                              {(lf.size_bytes / 1024 / 1024).toFixed(1)}MB
                            </span>
                          </div>
                        ))}
                        {allLogFiles.length > 20 && (
                          <div className="text-muted-foreground text-center">
                            ... and {allLogFiles.length - 20} more
                          </div>
                        )}
                      </div>
                    </div>
                  )}

                  {/* Config Files */}
                  {allConfigFiles.length > 0 && (
                    <div>
                      <h4 className="text-sm font-medium mb-2">
                        Config Files ({allConfigFiles.length})
                      </h4>
                      <div className="max-h-40 overflow-y-auto text-xs space-y-1 border rounded p-2">
                        {allConfigFiles.slice(0, 20).map((cf, i) => (
                          <div key={i} className="p-1 bg-muted rounded flex items-center justify-between">
                            <span className="font-mono truncate flex-1" title={cf.path}>
                              {cf.path.split('/').pop()}
                            </span>
                            {cf.endpoints > 0 && (
                              <Badge variant="secondary" className="text-[10px] ml-2">
                                {cf.endpoints} endpoints
                              </Badge>
                            )}
                          </div>
                        ))}
                        {allConfigFiles.length > 20 && (
                          <div className="text-muted-foreground text-center">
                            ... and {allConfigFiles.length - 20} more
                          </div>
                        )}
                      </div>
                    </div>
                  )}

                  {/* Listeners */}
                  {reportDetail.report?.listeners && (
                    <div>
                      <h4 className="text-sm font-medium mb-2">
                        Listeners ({reportDetail.report.listeners.length})
                      </h4>
                      <div className="max-h-40 overflow-y-auto text-xs space-y-1 border rounded p-2">
                        {reportDetail.report.listeners.map((l, i) => (
                          <div key={i} className="p-1 bg-muted rounded flex items-center justify-between">
                            <span className="font-mono">:{l.port}</span>
                            <span className="text-muted-foreground">
                              {l.process_name || 'unknown'}
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Connections */}
                  {reportDetail.report?.connections && reportDetail.report.connections.length > 0 && (
                    <div>
                      <h4 className="text-sm font-medium mb-2">
                        Outbound Connections ({reportDetail.report.connections.length})
                      </h4>
                      <div className="max-h-40 overflow-y-auto text-xs space-y-1 border rounded p-2">
                        {reportDetail.report.connections.slice(0, 20).map((c, i) => (
                          <div key={i} className="p-1 bg-muted rounded">
                            <span className="font-mono">
                              → {c.remote_addr}:{c.remote_port}
                            </span>
                            <span className="text-muted-foreground ml-2">
                              ({c.process_name || 'unknown'})
                            </span>
                          </div>
                        ))}
                        {reportDetail.report.connections.length > 20 && (
                          <div className="text-muted-foreground text-center">
                            ... and {reportDetail.report.connections.length - 20} more
                          </div>
                        )}
                      </div>
                    </div>
                  )}

                  {/* Firewall Rules */}
                  {reportDetail.report?.firewall_rules && reportDetail.report.firewall_rules.length > 0 && (
                    <div>
                      <h4 className="text-sm font-medium mb-2">
                        Firewall Rules ({reportDetail.report.firewall_rules.length})
                      </h4>
                      <div className="max-h-40 overflow-y-auto text-xs space-y-1 border rounded p-2">
                        {reportDetail.report.firewall_rules.slice(0, 20).map((r, i) => (
                          <div key={i} className="p-1 bg-muted rounded flex items-center justify-between">
                            <span className="font-mono">{r.name}</span>
                            <Badge
                              variant={r.action === 'allow' ? 'default' : 'destructive'}
                              className="text-[10px]"
                            >
                              {r.action} {r.local_port ? `:${r.local_port}` : ''}
                            </Badge>
                          </div>
                        ))}
                        {reportDetail.report.firewall_rules.length > 20 && (
                          <div className="text-muted-foreground text-center">
                            ... and {reportDetail.report.firewall_rules.length - 20} more
                          </div>
                        )}
                      </div>
                    </div>
                  )}
                </div>

                {/* Raw JSON button */}
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    // Export clean data structure
                    const exportData = {
                      id: reportDetail.id,
                      agent_id: reportDetail.agent_id,
                      hostname: reportDetail.hostname,
                      scanned_at: reportDetail.scanned_at,
                      report: reportDetail.report,
                    };
                    const blob = new Blob([JSON.stringify(exportData, null, 2)], { type: 'application/json' });
                    const url = URL.createObjectURL(blob);
                    const a = document.createElement('a');
                    a.href = url;
                    a.download = `discovery-${reportDetail.hostname}-${new Date(reportDetail.scanned_at).toISOString().split('T')[0]}.json`;
                    a.click();
                    URL.revokeObjectURL(url);
                  }}
                  className="w-full"
                >
                  <FileJson className="h-4 w-4 mr-2" />
                  Download Raw JSON
                </Button>
              </div>
            ) : (
              <p className="text-sm text-muted-foreground text-center py-8">
                Select a report to view details
              </p>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Phase 1: Scan — Select agents + trigger discovery
// ---------------------------------------------------------------------------

function ScanPhase() {
  const navigate = useNavigate();
  const {
    selectedAgentIds,
    setSelectedAgentIds,
    toggleAgentId,
    setCorrelationResult,
    setPhase,
    reset,
  } = useDiscoveryStore();

  const [showAgentPanel, setShowAgentPanel] = useState(false);
  // Snapshot timestamp for stale calculations (avoids impure Date.now() in render)
  const [now] = useState(() => Date.now());

  const { data: agentsData } = useAgents();
  const { data: reports } = useDiscoveryReports();
  const triggerAll = useTriggerAllScans();
  const correlate = useCorrelate();

  const agents: Agent[] = useMemo(() => {
    return Array.isArray(agentsData)
      ? agentsData
      : (agentsData as unknown as { agents?: Agent[] })?.agents || [];
  }, [agentsData]);

  const agentIdsWithReports = new Set(reports?.map((r) => r.agent_id) || []);

  // Count stale agents (not seen for 7+ days)
  const staleAgentCount = useMemo(() => agents.filter((a) => {
    if (!a.last_heartbeat_at) return true;
    const days = (now - new Date(a.last_heartbeat_at).getTime()) / (1000 * 60 * 60 * 24);
    return days > 7;
  }).length, [agents, now]);

  const selectAll = () => setSelectedAgentIds(agents.map((a) => a.id));

  const handleScan = async () => {
    if (selectedAgentIds.length === 0) return;
    await triggerAll.mutateAsync();
  };

  const handleAnalyze = async () => {
    if (selectedAgentIds.length === 0) return;
    const result = await correlate.mutateAsync({ agent_ids: selectedAgentIds });
    setCorrelationResult(result);

    // Auto-enable services with high confidence (recognized or likely)
    // The store's setCorrelationResult already enables all, but we want to
    // selectively disable low-confidence ones for the map-first UX
    const enabled = new Set<number>();
    result.services.forEach((svc, i) => {
      const confidence = classifyConfidence(svc);
      // Auto-include recognized and likely services
      if (confidence === 'recognized' || confidence === 'likely') {
        enabled.add(i);
      }
    });
    // Update store with filtered enabled set
    useDiscoveryStore.setState({ enabledServiceIndices: enabled });

    // Go directly to map phase (skip triage)
    setPhase('map');
  };

  const isScanning = triggerAll.isPending || correlate.isPending;

  return (
    <div className="space-y-6 relative">
      {/* Matrix rain background during scanning */}
      {isScanning && (
        <div className="absolute inset-0 -m-6 overflow-hidden pointer-events-none z-0">
          <MatrixRain opacity={0.08} color="#22c55e" />
        </div>
      )}

      {isScanning && (
        <div className="relative z-10 text-center text-emerald-600 font-medium">
          {triggerAll.isPending ? 'Scanning agents...' : 'Analyzing topology...'}
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 relative z-10">
        {/* Agent selection */}
        <Card>
          <CardContent className="p-6 space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="font-semibold text-lg flex items-center gap-2">
                <Server className="h-5 w-5" />
                Agents
              </h3>
              <div className="flex items-center gap-2">
                {staleAgentCount > 0 && (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => setShowAgentPanel(true)}
                    className="text-xs gap-1 text-amber-600 border-amber-300 hover:bg-amber-50"
                  >
                    <AlertTriangle className="h-3.5 w-3.5" />
                    {staleAgentCount} Stale
                  </Button>
                )}
                <Button size="sm" variant="outline" onClick={selectAll} className="text-xs">
                  Select All
                </Button>
              </div>
            </div>

            {agents.length === 0 ? (
              <p className="text-sm text-muted-foreground py-4 text-center">
                No agents connected. Deploy agents on your servers first.
              </p>
            ) : (
              <div className="space-y-1 max-h-[400px] overflow-y-auto">
                {agents.map((agent) => {
                  const checked = selectedAgentIds.includes(agent.id);
                  const hasReport = agentIdsWithReports.has(agent.id);
                  const isConnected = agent.connected;
                  const gatewayConnected = agent.gateway_connected;
                  const lastSeen = agent.last_heartbeat_at
                    ? new Date(agent.last_heartbeat_at)
                    : null;
                  const daysSinceHeartbeat = lastSeen
                    ? Math.floor((now - lastSeen.getTime()) / (1000 * 60 * 60 * 24))
                    : null;
                  const isStale = daysSinceHeartbeat !== null && daysSinceHeartbeat > 7;

                  return (
                    <label
                      key={agent.id}
                      className={`flex items-center gap-3 p-2 rounded-md hover:bg-accent cursor-pointer ${isStale ? 'opacity-60' : ''}`}
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => toggleAgentId(agent.id)}
                        className="h-4 w-4 rounded border-gray-300 text-primary focus:ring-primary"
                      />
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="text-sm font-medium">{agent.hostname || agent.id}</span>
                          <div
                            className={`w-2 h-2 rounded-full ${isConnected ? 'bg-emerald-500' : 'bg-slate-400'}`}
                            title={isConnected ? 'Connected' : 'Disconnected'}
                          />
                        </div>
                        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                          {agent.gateway_name ? (
                            <>
                              <span className="font-medium text-foreground/70">{agent.gateway_name}</span>
                              {agent.gateway_zone && (
                                <span className="text-muted-foreground/60">({agent.gateway_zone})</span>
                              )}
                              <span
                                className={`w-1.5 h-1.5 rounded-full ${gatewayConnected ? 'bg-emerald-400' : 'bg-slate-300'}`}
                                title={gatewayConnected ? 'Gateway connected' : 'Gateway disconnected'}
                              />
                            </>
                          ) : (
                            <span className="truncate">{agent.id}</span>
                          )}
                        </div>
                        {isStale && daysSinceHeartbeat !== null && (
                          <span className="text-[10px] text-amber-600">
                            Last seen {daysSinceHeartbeat} days ago
                          </span>
                        )}
                      </div>
                      {hasReport && (
                        <Badge variant="secondary" className="text-[10px]">
                          <CheckCircle className="h-3 w-3 mr-0.5 text-emerald-500" />
                          Scanned
                        </Badge>
                      )}
                    </label>
                  );
                })}
              </div>
            )}
          </CardContent>
        </Card>

        {/* Actions */}
        <Card>
          <CardContent className="p-6 space-y-4">
            <h3 className="font-semibold text-lg flex items-center gap-2">
              <Network className="h-5 w-5" />
              Actions
            </h3>

            <div className="space-y-3">
              <div className="text-sm text-muted-foreground">
                <strong>{selectedAgentIds.length}</strong> agent{selectedAgentIds.length !== 1 ? 's' : ''} selected
              </div>

              <Button
                onClick={handleScan}
                disabled={selectedAgentIds.length === 0 || triggerAll.isPending}
                className="w-full gap-2"
                variant="outline"
              >
                {triggerAll.isPending ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <Radar className="h-4 w-4" />
                )}
                Scan All Agents
              </Button>

              <Button
                onClick={handleAnalyze}
                disabled={selectedAgentIds.length === 0 || correlate.isPending}
                className="w-full gap-2"
              >
                {correlate.isPending ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <ArrowRight className="h-4 w-4" />
                )}
                Analyze Topology
              </Button>

              {correlate.isError && (
                <p className="text-xs text-destructive">
                  Correlation failed. Make sure agents have been scanned first.
                </p>
              )}

              <div className="text-xs text-muted-foreground pt-2 border-t border-border">
                Tip: First "Scan All Agents" to collect fresh data, then "Analyze Topology"
                to visualize the cross-host service map.
              </div>

              <Button
                variant="ghost"
                onClick={() => {
                  reset();
                  navigate('/');
                }}
                className="w-full gap-2 text-muted-foreground hover:text-destructive"
              >
                <X className="h-4 w-4" />
                Cancel Discovery
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Stale agents management panel */}
      <AgentManagementPanel open={showAgentPanel} onClose={() => setShowAgentPanel(false)} />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Phase 2: Map — Full-screen interactive topology map with inline refinement
// ---------------------------------------------------------------------------

function MapPhase() {
  const selectedServiceIndex = useDiscoveryStore((s) => s.selectedServiceIndex);

  return (
    <div className="h-[calc(100vh-7rem)] -m-6 flex">
      {/* Left sidebar */}
      <LayerSidebar />

      {/* Map canvas */}
      <div className="flex-1 relative">
        <TopologyToolbar />
        <TopologyMap />
      </div>

      {/* Right detail panel */}
      {selectedServiceIndex !== null && <ServiceDetailPanel />}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Phase 3: Done — Success + link
// ---------------------------------------------------------------------------

function DonePhase() {
  const navigate = useNavigate();
  const { createdAppId, appName, reset } = useDiscoveryStore();

  return (
    <div className="flex items-center justify-center min-h-[60vh]">
      <Card className="max-w-md w-full">
        <CardContent className="p-8 text-center space-y-4">
          <div className="flex justify-center">
            <div className="w-16 h-16 rounded-full bg-emerald-100 flex items-center justify-center">
              <Rocket className="h-8 w-8 text-emerald-600" />
            </div>
          </div>
          <h2 className="text-xl font-bold">Application Created!</h2>
          <p className="text-muted-foreground">
            <strong>{appName}</strong> has been created from the discovered topology.
            Components are ready with operational commands.
          </p>
          <div className="flex gap-3 justify-center pt-2">
            {createdAppId && (
              <Button onClick={() => navigate(`/apps/${createdAppId}`)} className="gap-2">
                <ArrowRight className="h-4 w-4" />
                View Application
              </Button>
            )}
            <Button variant="outline" onClick={reset} className="gap-2">
              <RotateCcw className="h-4 w-4" />
              Discover More
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
