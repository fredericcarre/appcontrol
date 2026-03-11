import { useState, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
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
} from 'lucide-react';
import {
  useDiscoveryReports,
  useTriggerAllScans,
  useCorrelate,
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
import { TriagePhase } from '@/components/discovery/TriagePhase';

// ---------------------------------------------------------------------------
// Main Page — 3-phase flow
// ---------------------------------------------------------------------------

export function DiscoveryPage() {
  const phase = useDiscoveryStore((s) => s.phase);
  const getTriageProgress = useDiscoveryStore((s) => s.getTriageProgress);
  const triageProgress = getTriageProgress();

  return (
    <div className="flex flex-col h-full">
      {/* Stepper - hidden in topology phase for more space */}
      {phase !== 'topology' && (
        <div className="mb-6 py-4 border-b border-border">
          <DiscoveryStepper currentPhase={phase} triageProgress={triageProgress} />
        </div>
      )}

      {/* Phase content */}
      {phase === 'scan' && <ScanPhase />}
      {phase === 'triage' && <TriagePhase />}
      {phase === 'topology' && <TopologyPhase />}
      {phase === 'done' && <DonePhase />}
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
    setPhase('triage');
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

      <div className="relative z-10">
        <h1 className="text-2xl font-bold flex items-center gap-2">
          <Radar className={`h-6 w-6 ${isScanning ? 'animate-pulse' : ''}`} />
          Discovery
          {isScanning && (
            <span className="text-sm font-normal text-emerald-600 ml-2">
              {triggerAll.isPending ? 'Scanning...' : 'Analyzing...'}
            </span>
          )}
        </h1>
        <p className="text-muted-foreground mt-1">
          Select agents to scan, then analyze the topology of your infrastructure.
        </p>
      </div>

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
// Phase 2: Topology — Full-screen interactive map
// ---------------------------------------------------------------------------

function TopologyPhase() {
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
