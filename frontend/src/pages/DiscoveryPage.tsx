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

// ---------------------------------------------------------------------------
// Main Page — 3-phase flow
// ---------------------------------------------------------------------------

export function DiscoveryPage() {
  const phase = useDiscoveryStore((s) => s.phase);

  return (
    <>
      {phase === 'scan' && <ScanPhase />}
      {phase === 'topology' && <TopologyPhase />}
      {phase === 'done' && <DonePhase />}
    </>
  );
}

// ---------------------------------------------------------------------------
// Phase 1: Scan — Select agents + trigger discovery
// ---------------------------------------------------------------------------

function ScanPhase() {
  const {
    selectedAgentIds,
    setSelectedAgentIds,
    toggleAgentId,
    setCorrelationResult,
    setPhase,
  } = useDiscoveryStore();

  const { data: agentsData } = useAgents();
  const { data: reports } = useDiscoveryReports();
  const triggerAll = useTriggerAllScans();
  const correlate = useCorrelate();

  const agents: Agent[] = Array.isArray(agentsData)
    ? agentsData
    : (agentsData as unknown as { agents?: Agent[] })?.agents || [];

  const agentIdsWithReports = new Set(reports?.map((r) => r.agent_id) || []);

  const selectAll = () => setSelectedAgentIds(agents.map((a) => a.id));

  const handleScan = async () => {
    if (selectedAgentIds.length === 0) return;
    await triggerAll.mutateAsync();
  };

  const handleAnalyze = async () => {
    if (selectedAgentIds.length === 0) return;
    const result = await correlate.mutateAsync({ agent_ids: selectedAgentIds });
    setCorrelationResult(result);
    setPhase('topology');
  };

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold flex items-center gap-2">
          <Radar className="h-6 w-6" />
          Discovery
        </h1>
        <p className="text-muted-foreground mt-1">
          Select agents to scan, then analyze the topology of your infrastructure.
        </p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Agent selection */}
        <Card>
          <CardContent className="p-6 space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="font-semibold text-lg flex items-center gap-2">
                <Server className="h-5 w-5" />
                Agents
              </h3>
              <Button size="sm" variant="outline" onClick={selectAll} className="text-xs">
                Select All
              </Button>
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
                  return (
                    <label
                      key={agent.id}
                      className="flex items-center gap-3 p-2 rounded-md hover:bg-accent cursor-pointer"
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
                          <div className="w-2 h-2 rounded-full bg-emerald-500" />
                        </div>
                        <span className="text-xs text-muted-foreground truncate block">
                          {agent.id}
                        </span>
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
            </div>
          </CardContent>
        </Card>
      </div>
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
