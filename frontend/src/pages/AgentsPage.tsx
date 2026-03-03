import { useState, useMemo } from 'react';
import { useAgents, useBlockAgent, useUnblockAgent, type Agent } from '@/api/agents';
import { useGateways } from '@/api/gateways';
import { useAuthStore } from '@/stores/auth';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from '@/components/ui/table';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Server,
  Wifi,
  WifiOff,
  MoreHorizontal,
  ShieldAlert,
  Search,
  Network,
  Circle,
  Cpu,
  HardDrive,
  MemoryStick,
  ChevronDown,
  ChevronRight,
  Activity,
  Terminal,
} from 'lucide-react';
import { TerminalModal } from '@/components/terminal/TerminalModal';
import { AgentMetricsChart } from '@/components/agents/AgentMetricsChart';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';

export function AgentsPage() {
  const user = useAuthStore((s) => s.user);
  const isAdmin = user?.role === 'admin';
  const { data: agents, isLoading } = useAgents();
  const { data: gateways } = useGateways();
  const blockAgent = useBlockAgent();
  const unblockAgent = useUnblockAgent();

  // Search and filters
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<'all' | 'connected' | 'disconnected'>('all');
  const [gatewayFilter, setGatewayFilter] = useState<string>('all');

  // Block/unblock confirmation dialogs
  const [blockConfirm, setBlockConfirm] = useState<Agent | null>(null);
  const [unblockConfirm, setUnblockConfirm] = useState<Agent | null>(null);

  // Expanded agent for metrics view
  const [expandedAgentId, setExpandedAgentId] = useState<string | null>(null);

  // Terminal modal state
  const [terminalAgent, setTerminalAgent] = useState<Agent | null>(null);

  // Get unique gateways for the filter dropdown
  const gatewayOptions = useMemo(() => {
    if (!gateways) return [];
    return gateways.map((g) => ({ id: g.id, name: g.name, zone: g.zone }));
  }, [gateways]);

  // Filter agents
  const filteredAgents = useMemo(() => {
    if (!agents) return [];
    return agents.filter((agent) => {
      // Search filter (hostname or ID)
      if (search) {
        const searchLower = search.toLowerCase();
        const matchesHostname = agent.hostname?.toLowerCase().includes(searchLower);
        const matchesId = agent.id?.toLowerCase().includes(searchLower);
        if (!matchesHostname && !matchesId) return false;
      }
      // Status filter
      if (statusFilter === 'connected' && !agent.connected) return false;
      if (statusFilter === 'disconnected' && agent.connected) return false;
      // Gateway filter
      if (gatewayFilter === 'no-gateway' && agent.gateway_id) return false;
      if (gatewayFilter !== 'all' && gatewayFilter !== 'no-gateway' && agent.gateway_id !== gatewayFilter)
        return false;
      return true;
    });
  }, [agents, search, statusFilter, gatewayFilter]);

  const handleBlock = async () => {
    if (!blockConfirm) return;
    await blockAgent.mutateAsync(blockConfirm.id);
    setBlockConfirm(null);
  };

  const handleUnblock = async () => {
    if (!unblockConfirm) return;
    await unblockAgent.mutateAsync(unblockConfirm.id);
    setUnblockConfirm(null);
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin h-8 w-8 border-2 border-primary border-t-transparent rounded-full" />
      </div>
    );
  }

  const agentList: Agent[] = agents || [];
  const connectedCount = agentList.filter((a) => a.connected).length;

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Agents</h1>
        <div className="text-sm text-muted-foreground">
          {connectedCount}/{agentList.length} connected
        </div>
      </div>

      {/* Search and Filters */}
      <div className="flex flex-wrap items-center gap-3">
        <div className="relative flex-1 min-w-[200px] max-w-sm">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Search by hostname or ID..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="pl-9"
          />
        </div>
        <Select value={statusFilter} onValueChange={(v) => setStatusFilter(v as typeof statusFilter)}>
          <SelectTrigger className="w-[150px]">
            <SelectValue placeholder="Status" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All Status</SelectItem>
            <SelectItem value="connected">Connected</SelectItem>
            <SelectItem value="disconnected">Disconnected</SelectItem>
          </SelectContent>
        </Select>
        <Select value={gatewayFilter} onValueChange={setGatewayFilter}>
          <SelectTrigger className="w-[180px]">
            <SelectValue placeholder="Gateway" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All Gateways</SelectItem>
            <SelectItem value="no-gateway">No Gateway</SelectItem>
            {gatewayOptions.map((gw) => (
              <SelectItem key={gw.id} value={gw.id}>
                {gw.name} ({gw.zone})
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-[40px]"></TableHead>
                <TableHead>Agent</TableHead>
                <TableHead>Hostname</TableHead>
                <TableHead>Gateway</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>System</TableHead>
                <TableHead>Version</TableHead>
                <TableHead>Last Heartbeat</TableHead>
                {isAdmin && <TableHead className="w-[50px]"></TableHead>}
              </TableRow>
            </TableHeader>
            <TableBody>
              {!filteredAgents.length ? (
                <TableRow>
                  <TableCell colSpan={isAdmin ? 9 : 8} className="text-center text-muted-foreground py-8">
                    {agentList.length === 0 ? 'No agents registered' : 'No agents match filters'}
                  </TableCell>
                </TableRow>
              ) : (
                filteredAgents.map((agent) => (
                  <>
                  <TableRow key={agent.id} className="cursor-pointer hover:bg-muted/50" onClick={() => setExpandedAgentId(expandedAgentId === agent.id ? null : agent.id)}>
                    <TableCell className="w-[40px]">
                      <Button variant="ghost" size="icon" className="h-6 w-6">
                        {expandedAgentId === agent.id ? (
                          <ChevronDown className="h-4 w-4" />
                        ) : (
                          <ChevronRight className="h-4 w-4" />
                        )}
                      </Button>
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <Server className="h-4 w-4 text-muted-foreground" />
                        <span className="font-medium font-mono text-xs">{agent.id?.slice(0, 8)}</span>
                      </div>
                    </TableCell>
                    <TableCell>{agent.hostname || '-'}</TableCell>
                    <TableCell>
                      {agent.gateway_name ? (
                        <div className="flex items-center gap-2">
                          <Network className="h-3 w-3 text-muted-foreground" />
                          <span className="text-sm">{agent.gateway_name}</span>
                          <Badge variant="outline" className="text-xs">
                            {agent.gateway_zone}
                          </Badge>
                          <span title={agent.gateway_connected ? 'Gateway online' : 'Gateway offline'}>
                            {agent.gateway_connected ? (
                              <Circle className="h-2 w-2 fill-green-500 text-green-500" />
                            ) : (
                              <Circle className="h-2 w-2 fill-red-500 text-red-500" />
                            )}
                          </span>
                        </div>
                      ) : (
                        <span className="text-muted-foreground text-sm">-</span>
                      )}
                    </TableCell>
                    <TableCell>
                      {agent.connected ? (
                        <Badge variant="running" className="gap-1">
                          <Wifi className="h-3 w-3" /> Connected
                        </Badge>
                      ) : (
                        <Badge variant="stopped" className="gap-1">
                          <WifiOff className="h-3 w-3" /> Disconnected
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell>
                      {agent.os_name ? (
                        <TooltipProvider>
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <div className="flex items-center gap-1 text-sm cursor-help">
                                <span>{agent.os_name}</span>
                                {agent.cpu_arch && (
                                  <Badge variant="outline" className="text-xs font-normal">
                                    {agent.cpu_arch}
                                  </Badge>
                                )}
                              </div>
                            </TooltipTrigger>
                            <TooltipContent side="bottom" className="text-xs">
                              <div className="space-y-1">
                                <div className="flex items-center gap-2">
                                  <span className="text-muted-foreground">OS:</span>
                                  <span>{agent.os_name} {agent.os_version}</span>
                                </div>
                                {agent.cpu_cores && (
                                  <div className="flex items-center gap-2">
                                    <Cpu className="h-3 w-3" />
                                    <span>{agent.cpu_cores} cores</span>
                                  </div>
                                )}
                                {agent.total_memory_mb && (
                                  <div className="flex items-center gap-2">
                                    <MemoryStick className="h-3 w-3" />
                                    <span>{Math.round(agent.total_memory_mb / 1024)} GB RAM</span>
                                  </div>
                                )}
                                {agent.disk_total_gb && (
                                  <div className="flex items-center gap-2">
                                    <HardDrive className="h-3 w-3" />
                                    <span>{agent.disk_total_gb} GB disk</span>
                                  </div>
                                )}
                              </div>
                            </TooltipContent>
                          </Tooltip>
                        </TooltipProvider>
                      ) : (
                        <span className="text-muted-foreground">-</span>
                      )}
                    </TableCell>
                    <TableCell className="text-muted-foreground">{agent.version || '-'}</TableCell>
                    <TableCell className="text-muted-foreground text-sm">
                      {agent.last_heartbeat_at ? new Date(agent.last_heartbeat_at).toLocaleString() : '-'}
                    </TableCell>
                    {isAdmin && (
                      <TableCell onClick={(e) => e.stopPropagation()}>
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="icon" className="h-8 w-8" onClick={(e) => e.stopPropagation()}>
                              <MoreHorizontal className="h-4 w-4" />
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            <DropdownMenuItem disabled>View Details</DropdownMenuItem>
                            {agent.connected && (
                              <DropdownMenuItem
                                onClick={(e) => { e.stopPropagation(); setTerminalAgent(agent); }}
                              >
                                <Terminal className="h-4 w-4 mr-2" />
                                Open Terminal
                              </DropdownMenuItem>
                            )}
                            <DropdownMenuSeparator />
                            {agent.is_active ? (
                              <DropdownMenuItem
                                onClick={(e) => { e.stopPropagation(); setBlockConfirm(agent); }}
                                className="text-destructive focus:text-destructive"
                              >
                                <ShieldAlert className="h-4 w-4 mr-2" />
                                Block Agent
                              </DropdownMenuItem>
                            ) : (
                              <DropdownMenuItem
                                onClick={(e) => { e.stopPropagation(); setUnblockConfirm(agent); }}
                                className="text-green-600 focus:text-green-600"
                              >
                                <ShieldAlert className="h-4 w-4 mr-2" />
                                Unblock Agent
                              </DropdownMenuItem>
                            )}
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </TableCell>
                    )}
                  </TableRow>
                  {expandedAgentId === agent.id && (
                    <TableRow>
                      <TableCell colSpan={isAdmin ? 9 : 8} className="p-4 bg-muted/30">
                        <AgentMetricsChart agentId={agent.id} hostname={agent.hostname || 'Unknown'} />
                      </TableCell>
                    </TableRow>
                  )}
                  </>
                ))
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* Block Agent Confirmation Dialog */}
      <Dialog open={!!blockConfirm} onOpenChange={(open) => !open && setBlockConfirm(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <ShieldAlert className="h-5 w-5 text-destructive" />
              Block Agent
            </DialogTitle>
            <DialogDescription>
              Are you sure you want to block the agent{' '}
              <span className="font-medium">{blockConfirm?.hostname}</span>? It will be immediately
              disconnected and unable to reconnect until unblocked.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setBlockConfirm(null)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleBlock} disabled={blockAgent.isPending}>
              {blockAgent.isPending ? 'Blocking...' : 'Block Agent'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Unblock Agent Confirmation Dialog */}
      <Dialog open={!!unblockConfirm} onOpenChange={(open) => !open && setUnblockConfirm(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <ShieldAlert className="h-5 w-5 text-green-600" />
              Unblock Agent
            </DialogTitle>
            <DialogDescription>
              Are you sure you want to unblock the agent{' '}
              <span className="font-medium">{unblockConfirm?.hostname}</span>? It will be allowed to
              reconnect to the platform.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={(e) => { e.stopPropagation(); setUnblockConfirm(null); }}>
              Cancel
            </Button>
            <Button onClick={(e) => { e.stopPropagation(); handleUnblock(); }} disabled={unblockAgent.isPending} className="bg-green-600 hover:bg-green-700">
              {unblockAgent.isPending ? 'Unblocking...' : 'Unblock Agent'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Terminal Modal */}
      {terminalAgent && (
        <TerminalModal
          agentId={terminalAgent.id}
          agentHostname={terminalAgent.hostname || 'Unknown'}
          open={!!terminalAgent}
          onClose={() => setTerminalAgent(null)}
        />
      )}
    </div>
  );
}
