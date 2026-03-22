import { useState, useMemo } from 'react';
import {
  useGatewaySites,
  useGatewayAgents,
  useActivateGateway,
  useSetGatewayPrimary,
  useDeleteGateway,
  useBlockGateway,
  useUpdateGateway,
  type Gateway,
  type SiteSummary,
  type GatewayAgent,
} from '@/api/gateways';
import { useSites } from '@/api/sites';
import { useBlockAgent } from '@/api/agents';
import { useAuthStore } from '@/stores/auth';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
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
  Network,
  Server,
  Wifi,
  WifiOff,
  ChevronRight,
  ChevronDown,
  MoreHorizontal,
  Play,
  Trash2,
  ShieldAlert,
  Star,
  AlertTriangle,
  Clock,
  Plus,
  Copy,
  CheckCircle2,
  Search,
  FileText,
  MapPin,
} from 'lucide-react';
import { LogViewerModal } from '@/components/logs/LogViewerModal';

function formatTimeAgo(dateStr: string | null): string {
  if (!dateStr) return 'Never';
  const date = new Date(dateStr);
  const seconds = Math.floor((Date.now() - date.getTime()) / 1000);
  if (seconds < 60) return `${seconds}s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}

function getRoleBadge(gateway: Gateway, isSingleGateway: boolean = false) {
  if (isSingleGateway && gateway.role === 'failover_active') {
    return (
      <Badge variant="default" className="gap-1 bg-green-600 hover:bg-green-700">
        Active
      </Badge>
    );
  }

  switch (gateway.role) {
    case 'primary':
      return (
        <Badge variant="default" className="gap-1 bg-blue-600 hover:bg-blue-700">
          <Star className="h-3 w-3" /> Primary
        </Badge>
      );
    case 'primary_offline':
      return (
        <Badge variant="destructive" className="gap-1">
          <Star className="h-3 w-3" /> Primary (Offline)
        </Badge>
      );
    case 'failover_active':
      return (
        <Badge variant="default" className="gap-1 bg-orange-600 hover:bg-orange-700">
          <AlertTriangle className="h-3 w-3" /> Failover Active
        </Badge>
      );
    case 'standby':
      return (
        <Badge variant="secondary" className="gap-1">
          Standby
        </Badge>
      );
    case 'standby_offline':
      return (
        <Badge variant="outline" className="gap-1">
          <WifiOff className="h-3 w-3" /> Standby (Offline)
        </Badge>
      );
    default:
      return <Badge variant="secondary">{gateway.role}</Badge>;
  }
}

function getConnectionBadge(gateway: Gateway) {
  if (gateway.status === 'suspended') {
    return <Badge variant="secondary">Suspended</Badge>;
  }
  if (gateway.connected) {
    return (
      <Badge variant="default" className="gap-1 bg-green-600 hover:bg-green-700">
        <Wifi className="h-3 w-3" /> Online
      </Badge>
    );
  }
  return (
    <Badge variant="outline" className="gap-1 text-muted-foreground">
      <WifiOff className="h-3 w-3" /> Offline
    </Badge>
  );
}

interface AgentItemProps {
  agent: GatewayAgent;
  isAdmin: boolean;
  onBlock: (agent: GatewayAgent) => void;
}

function AgentItem({ agent, isAdmin, onBlock }: AgentItemProps) {
  return (
    <div className="flex items-center gap-3 text-sm py-1.5">
      <Server className="h-3 w-3 text-muted-foreground" />
      <span className="font-mono text-xs">{agent.id.slice(0, 8)}</span>
      <span className="flex-1">{agent.hostname}</span>
      {agent.connected ? (
        <Badge variant="default" className="text-xs gap-1 bg-green-600">
          <Wifi className="h-2.5 w-2.5" /> Connected
        </Badge>
      ) : (
        <Badge variant="outline" className="text-xs gap-1 text-muted-foreground">
          <WifiOff className="h-2.5 w-2.5" /> Disconnected
        </Badge>
      )}
      {agent.last_heartbeat_at && (
        <span className="text-xs text-muted-foreground">{formatTimeAgo(agent.last_heartbeat_at)}</span>
      )}
      {isAdmin && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon" className="h-6 w-6">
              <MoreHorizontal className="h-3 w-3" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem
              onClick={() => onBlock(agent)}
              className="text-destructive focus:text-destructive"
            >
              <ShieldAlert className="h-4 w-4 mr-2" />
              Block Agent
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}

interface GatewayItemProps {
  gateway: Gateway;
  isAdmin: boolean;
  isSingleGateway: boolean;
  isMutating: boolean;
  onActivate: (gateway: Gateway) => void;
  onSetPrimary: (gateway: Gateway) => void;
  onDelete: (gateway: Gateway) => void;
  onBlock: (gateway: Gateway) => void;
  onBlockAgent: (agent: GatewayAgent) => void;
  onViewLogs: (gateway: Gateway) => void;
  onAssignSite: (gateway: Gateway) => void;
}

function GatewayItem({
  gateway,
  isAdmin,
  isSingleGateway,
  isMutating,
  onActivate,
  onSetPrimary,
  onDelete,
  onBlock,
  onBlockAgent,
  onViewLogs,
  onAssignSite,
}: GatewayItemProps) {
  const [expanded, setExpanded] = useState(false);
  const { data: agents, isLoading } = useGatewayAgents(expanded ? gateway.id : '');

  const connectedAgents = agents?.filter((a) => a.connected).length ?? 0;
  const totalAgents = agents?.length ?? gateway.agent_count;

  return (
    <div className="border-l-2 border-muted ml-4">
      <div
        className="flex items-center gap-3 py-2 px-3 hover:bg-muted/50 cursor-pointer"
        onClick={() => setExpanded(!expanded)}
      >
        <Button variant="ghost" size="icon" className="h-6 w-6 shrink-0">
          {expanded ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
        </Button>
        <Network className="h-4 w-4 text-muted-foreground shrink-0" />
        <span className="font-medium">{gateway.name}</span>
        {gateway.site_code && (
          <Badge variant="outline" className="gap-1 text-xs">
            <MapPin className="h-3 w-3" />
            {gateway.site_code}
          </Badge>
        )}
        {gateway.version && (
          <span className="text-xs text-muted-foreground bg-muted px-1.5 py-0.5 rounded">
            v{gateway.version}
          </span>
        )}
        <div className="flex items-center gap-2 ml-auto">
          {getRoleBadge(gateway, isSingleGateway)}
          {getConnectionBadge(gateway)}
          <span className="text-xs text-muted-foreground flex items-center gap-1">
            <Clock className="h-3 w-3" />
            {formatTimeAgo(gateway.last_heartbeat_at)}
          </span>
          <span className="text-sm text-muted-foreground">
            {expanded ? `${connectedAgents}/${totalAgents}` : gateway.agent_count} agent
            {(expanded ? totalAgents : gateway.agent_count) !== 1 ? 's' : ''}
          </span>
          {isAdmin && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild onClick={(e) => e.stopPropagation()}>
                <Button variant="ghost" size="icon" className="h-8 w-8">
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {gateway.connected && (
                  <DropdownMenuItem onClick={() => onViewLogs(gateway)}>
                    <FileText className="h-4 w-4 mr-2" />
                    View Logs
                  </DropdownMenuItem>
                )}
                <DropdownMenuItem onClick={() => onAssignSite(gateway)} disabled={isMutating}>
                  <MapPin className="h-4 w-4 mr-2" />
                  Assign to Site
                </DropdownMenuItem>
                {!gateway.is_primary && (
                  <DropdownMenuItem onClick={() => onSetPrimary(gateway)} disabled={isMutating}>
                    <Star className="h-4 w-4 mr-2" />
                    Set as Primary
                  </DropdownMenuItem>
                )}
                {gateway.status === 'suspended' ? (
                  <DropdownMenuItem onClick={() => onActivate(gateway)} disabled={isMutating}>
                    <Play className="h-4 w-4 mr-2" />
                    Activate
                  </DropdownMenuItem>
                ) : (
                  <DropdownMenuItem
                    onClick={() => onBlock(gateway)}
                    disabled={isMutating}
                    className="text-destructive focus:text-destructive"
                  >
                    <ShieldAlert className="h-4 w-4 mr-2" />
                    Block Gateway
                  </DropdownMenuItem>
                )}
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  onClick={() => onDelete(gateway)}
                  disabled={isMutating}
                  className="text-destructive focus:text-destructive"
                >
                  <Trash2 className="h-4 w-4 mr-2" />
                  Delete
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          )}
        </div>
      </div>
      {expanded && (
        <div className="ml-10 mr-4 mb-2 p-3 bg-muted/30 rounded-md">
          <h4 className="text-sm font-medium mb-2 flex items-center gap-2">
            <Server className="h-4 w-4" /> Agents ({connectedAgents}/{totalAgents} connected)
          </h4>
          {isLoading ? (
            <div className="text-sm text-muted-foreground">Loading agents...</div>
          ) : !agents?.length ? (
            <div className="text-sm text-muted-foreground">No agents assigned to this gateway</div>
          ) : (
            <div className="space-y-1">
              {agents.map((agent) => (
                <AgentItem key={agent.id} agent={agent} isAdmin={isAdmin} onBlock={onBlockAgent} />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

interface SiteCardProps {
  site: SiteSummary;
  isAdmin: boolean;
  search: string;
  isMutating: boolean;
  onActivate: (gateway: Gateway) => void;
  onSetPrimary: (gateway: Gateway) => void;
  onDelete: (gateway: Gateway) => void;
  onBlock: (gateway: Gateway) => void;
  onBlockAgent: (agent: GatewayAgent) => void;
  onViewLogs: (gateway: Gateway) => void;
  onAssignSite: (gateway: Gateway) => void;
}

function SiteCard({
  site,
  isAdmin,
  search,
  isMutating,
  onActivate,
  onSetPrimary,
  onDelete,
  onBlock,
  onBlockAgent,
  onViewLogs,
  onAssignSite,
}: SiteCardProps) {
  const [collapsed, setCollapsed] = useState(false);
  const [showAddFailover, setShowAddFailover] = useState(false);
  const [copied, setCopied] = useState(false);

  const filteredGateways = useMemo(() => {
    if (!search) return site.gateways;
    const searchLower = search.toLowerCase();
    return site.gateways.filter((g) => g.name.toLowerCase().includes(searchLower));
  }, [site.gateways, search]);

  const connectedCount = site.gateways.filter((g) => g.connected).length;
  const showFailoverAlert = site.failover_active && site.gateway_count >= 2;
  const singleGateway = site.gateway_count === 1;

  // Use site_id if available for new gateways
  const siteIdParam = site.site_id ? `GATEWAY_SITE_ID=${site.site_id}` : `# No site_id yet - will be assigned via UI`;
  const failoverCommand = `GATEWAY_NAME=gateway-${site.site_code.toLowerCase()}-standby \\
${siteIdParam} \\
LISTEN_PORT=4443 \\
BACKEND_URL=wss://your-backend:443/ws/gateway \\
./appcontrol-gateway`;

  const copyCommand = () => {
    navigator.clipboard.writeText(failoverCommand.replace(/\\\n/g, ' '));
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  if (search && filteredGateways.length === 0) return null;

  return (
    <>
      <Card className={showFailoverAlert ? 'border-orange-300 dark:border-orange-800' : ''}>
        <CardHeader
          className="cursor-pointer hover:bg-muted/50 transition-colors"
          onClick={() => setCollapsed(!collapsed)}
        >
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <Button variant="ghost" size="icon" className="h-6 w-6">
                {collapsed ? <ChevronRight className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
              </Button>
              <MapPin className="h-4 w-4 text-blue-600" />
              <CardTitle className="text-lg">{site.site_name}</CardTitle>
              <Badge variant="secondary">
                {site.gateway_count} gateway{site.gateway_count !== 1 ? 's' : ''}
              </Badge>
              {singleGateway && (
                <Badge variant="outline" className="gap-1 text-muted-foreground">
                  No redundancy
                </Badge>
              )}
            </div>
            <div className="flex items-center gap-2">
              {showFailoverAlert && (
                <Badge variant="destructive" className="gap-1">
                  <AlertTriangle className="h-3 w-3" /> Failover Active
                </Badge>
              )}
              <span className="text-sm text-muted-foreground">
                {connectedCount}/{site.gateway_count} online
              </span>
            </div>
          </div>
        </CardHeader>
        {!collapsed && (
          <CardContent className="pt-0">
            {filteredGateways.map((gateway) => (
              <GatewayItem
                key={gateway.id}
                gateway={gateway}
                isAdmin={isAdmin}
                isSingleGateway={singleGateway}
                isMutating={isMutating}
                onActivate={onActivate}
                onSetPrimary={onSetPrimary}
                onDelete={onDelete}
                onBlock={onBlock}
                onBlockAgent={onBlockAgent}
                onViewLogs={onViewLogs}
                onAssignSite={onAssignSite}
              />
            ))}
            {singleGateway && !search && (
              <div className="mt-4 ml-4 p-3 border border-dashed rounded-md flex items-center justify-between">
                <div className="text-sm text-muted-foreground">
                  Add a standby gateway for automatic failover
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  className="gap-1"
                  onClick={(e) => {
                    e.stopPropagation();
                    setShowAddFailover(true);
                  }}
                >
                  <Plus className="h-4 w-4" /> Add Failover Gateway
                </Button>
              </div>
            )}
          </CardContent>
        )}
      </Card>

      <Dialog open={showAddFailover} onOpenChange={setShowAddFailover}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Plus className="h-5 w-5" />
              Add Failover Gateway to "{site.site_name}"
            </DialogTitle>
            <DialogDescription>
              Deploy a standby gateway on a separate server for automatic failover when the primary goes
              offline.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <h4 className="font-medium mb-2">1. Download the gateway binary</h4>
              <p className="text-sm text-muted-foreground">
                Download the same <code className="bg-muted px-1 rounded">appcontrol-gateway</code> binary
                used for the primary gateway.
              </p>
            </div>
            <div>
              <h4 className="font-medium mb-2">2. Copy certificates</h4>
              <p className="text-sm text-muted-foreground">
                Copy the TLS certificates from the primary gateway or generate new ones signed by the same
                PKI CA.
              </p>
            </div>
            <div>
              <h4 className="font-medium mb-2">3. Start with standby configuration</h4>
              <p className="text-sm text-muted-foreground mb-2">
                Assign the gateway to the same site with a different name:
              </p>
              <div className="relative">
                <pre className="bg-muted p-3 rounded-md text-sm font-mono overflow-x-auto">
                  {failoverCommand}
                </pre>
                <Button variant="ghost" size="sm" className="absolute top-2 right-2 gap-1" onClick={copyCommand}>
                  {copied ? (
                    <>
                      <CheckCircle2 className="h-4 w-4 text-green-500" /> Copied
                    </>
                  ) : (
                    <>
                      <Copy className="h-4 w-4" /> Copy
                    </>
                  )}
                </Button>
              </div>
            </div>
            <div>
              <h4 className="font-medium mb-2">4. Verify registration</h4>
              <p className="text-sm text-muted-foreground">
                The new gateway will appear here as "Standby". If the primary goes offline, it will
                automatically become "Failover Active".
              </p>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowAddFailover(false)}>
              Close
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

export function GatewaysPage() {
  const user = useAuthStore((s) => s.user);
  const isAdmin = user?.role === 'admin';
  const { data: sites, isLoading, refetch } = useGatewaySites();
  const { data: allSites } = useSites();
  const activateGateway = useActivateGateway();
  const setGatewayPrimary = useSetGatewayPrimary();
  const deleteGateway = useDeleteGateway();
  const blockGateway = useBlockGateway();
  const blockAgent = useBlockAgent();
  const updateGateway = useUpdateGateway();

  const isMutating =
    activateGateway.isPending ||
    setGatewayPrimary.isPending ||
    deleteGateway.isPending ||
    blockGateway.isPending ||
    blockAgent.isPending ||
    updateGateway.isPending;

  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<'all' | 'online' | 'offline'>('all');
  const [siteFilter, setSiteFilter] = useState<string>('all');

  const [deleteConfirm, setDeleteConfirm] = useState<Gateway | null>(null);
  const [blockGatewayConfirm, setBlockGatewayConfirm] = useState<Gateway | null>(null);
  const [blockAgentConfirm, setBlockAgentConfirm] = useState<GatewayAgent | null>(null);

  const [assignSiteGateway, setAssignSiteGateway] = useState<Gateway | null>(null);
  const [selectedSiteId, setSelectedSiteId] = useState<string>('unassigned');

  const [logsGateway, setLogsGateway] = useState<Gateway | null>(null);

  const siteOptions = useMemo(() => {
    if (!sites) return [];
    return sites.map((s) => ({
      id: s.site_id || s.site_code,
      name: s.site_name,
      code: s.site_code
    }));
  }, [sites]);

  const filteredSites = useMemo(() => {
    if (!sites) return [];
    return sites
      .filter((site) => {
        // Use site_id or site_code for filtering
        const siteKey = site.site_id || site.site_code;
        if (siteFilter !== 'all' && siteKey !== siteFilter) return false;
        if (statusFilter !== 'all') {
          const hasMatchingGateway = site.gateways.some((g) =>
            statusFilter === 'online' ? g.connected : !g.connected
          );
          if (!hasMatchingGateway) return false;
        }
        return true;
      })
      .map((site) => {
        if (statusFilter === 'all') return site;
        return {
          ...site,
          gateways: site.gateways.filter((g) =>
            statusFilter === 'online' ? g.connected : !g.connected
          ),
        };
      });
  }, [sites, statusFilter, siteFilter]);

  const handleActivate = async (gateway: Gateway) => {
    if (isMutating) return;
    await activateGateway.mutateAsync(gateway.id);
    refetch();
  };

  const handleSetPrimary = async (gateway: Gateway) => {
    if (isMutating) return;
    await setGatewayPrimary.mutateAsync(gateway.id);
    refetch();
  };

  const handleDelete = async () => {
    if (!deleteConfirm || isMutating) return;
    await deleteGateway.mutateAsync(deleteConfirm.id);
    setDeleteConfirm(null);
    refetch();
  };

  const handleBlockGateway = async () => {
    if (!blockGatewayConfirm || isMutating) return;
    await blockGateway.mutateAsync(blockGatewayConfirm.id);
    setBlockGatewayConfirm(null);
    refetch();
  };

  const handleBlockAgent = async () => {
    if (!blockAgentConfirm || isMutating) return;
    await blockAgent.mutateAsync(blockAgentConfirm.id);
    setBlockAgentConfirm(null);
    refetch();
  };

  const openAssignSite = (gateway: Gateway) => {
    setAssignSiteGateway(gateway);
    setSelectedSiteId(gateway.site_id || 'unassigned');
  };

  const handleAssignSite = async () => {
    if (!assignSiteGateway || isMutating) return;
    await updateGateway.mutateAsync({
      id: assignSiteGateway.id,
      site_id: selectedSiteId === 'unassigned' ? null : selectedSiteId,
    });
    setAssignSiteGateway(null);
    refetch();
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin h-8 w-8 border-2 border-primary border-t-transparent rounded-full" />
      </div>
    );
  }

  const siteList = sites || [];
  const totalGateways = siteList.reduce((sum, s) => sum + s.gateway_count, 0);
  const totalConnected = siteList.reduce(
    (sum, s) => sum + s.gateways.filter((g) => g.connected).length,
    0
  );
  const failoverSites = siteList.filter((s) => s.failover_active && s.gateway_count >= 2);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Gateways</h1>
        <div className="flex items-center gap-4">
          {failoverSites.length > 0 && (
            <Badge variant="destructive" className="gap-1">
              <AlertTriangle className="h-3 w-3" />
              {failoverSites.length} site{failoverSites.length !== 1 ? 's' : ''} in failover
            </Badge>
          )}
          <span className="text-sm text-muted-foreground">
            {totalConnected}/{totalGateways} gateways online
          </span>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <div className="relative flex-1 min-w-[200px] max-w-sm">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Search by gateway name..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="pl-9"
          />
        </div>
        <Select value={statusFilter} onValueChange={(v) => setStatusFilter(v as typeof statusFilter)}>
          <SelectTrigger className="w-[130px]">
            <SelectValue placeholder="Status" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All Status</SelectItem>
            <SelectItem value="online">Online</SelectItem>
            <SelectItem value="offline">Offline</SelectItem>
          </SelectContent>
        </Select>
        <Select value={siteFilter} onValueChange={setSiteFilter}>
          <SelectTrigger className="w-[180px]">
            <SelectValue placeholder="Site" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All Sites</SelectItem>
            {siteOptions.map((site) => (
              <SelectItem key={site.id || 'unassigned'} value={site.id || 'unassigned'}>
                <span className="flex items-center gap-2">
                  <MapPin className="h-3 w-3" />
                  {site.name} ({site.code})
                </span>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {!siteList.length ? (
        <Card>
          <CardContent className="flex flex-col items-center justify-center py-12 text-center">
            <Network className="h-12 w-12 text-muted-foreground mb-4" />
            <h3 className="font-medium text-lg mb-2">No Gateways Registered</h3>
            <p className="text-muted-foreground max-w-md">
              Start a gateway to see it here. Gateways relay agent connections to the backend and handle
              mTLS authentication.
            </p>
          </CardContent>
        </Card>
      ) : filteredSites.length === 0 ? (
        <Card>
          <CardContent className="flex flex-col items-center justify-center py-12 text-center">
            <Search className="h-12 w-12 text-muted-foreground mb-4" />
            <h3 className="font-medium text-lg mb-2">No Matches</h3>
            <p className="text-muted-foreground">No gateways match your search or filters.</p>
          </CardContent>
        </Card>
      ) : (
        <div className="space-y-4">
          {filteredSites.map((site) => (
            <SiteCard
              key={site.site_id || site.site_code}
              site={site}
              isAdmin={isAdmin}
              search={search}
              isMutating={isMutating}
              onActivate={handleActivate}
              onSetPrimary={handleSetPrimary}
              onDelete={setDeleteConfirm}
              onBlock={setBlockGatewayConfirm}
              onBlockAgent={setBlockAgentConfirm}
              onViewLogs={setLogsGateway}
              onAssignSite={openAssignSite}
            />
          ))}
        </div>
      )}

      <Card>
        <CardHeader>
          <CardTitle className="text-lg">About Sites & Failover</CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground space-y-2">
          <p>
            <strong>Sites</strong> represent physical or logical locations (datacenters) where infrastructure runs. Each site has one{' '}
            <strong>primary</strong> gateway and zero or more <strong>standby</strong> gateways for high availability.
          </p>
          <p>
            When the primary gateway goes offline, the standby with the lowest priority number takes over
            automatically (<strong>failover</strong>). Sites are also used for DR switchover — you can fail over
            entire applications from one site to another.
          </p>
        </CardContent>
      </Card>

      <Dialog open={!!deleteConfirm} onOpenChange={(open) => !open && setDeleteConfirm(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Trash2 className="h-5 w-5 text-destructive" />
              Delete Gateway
            </DialogTitle>
            <DialogDescription>
              Are you sure you want to delete the gateway{' '}
              <span className="font-medium">{deleteConfirm?.name}</span>
              {deleteConfirm?.site_name && (
                <> at site <span className="font-medium">{deleteConfirm?.site_name}</span></>
              )}
              ? This will disconnect all agents connected through this gateway.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteConfirm(null)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleDelete} disabled={deleteGateway.isPending}>
              {deleteGateway.isPending ? 'Deleting...' : 'Delete'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={!!blockGatewayConfirm} onOpenChange={(open) => !open && setBlockGatewayConfirm(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <ShieldAlert className="h-5 w-5 text-destructive" />
              Block Gateway
            </DialogTitle>
            <DialogDescription>
              Are you sure you want to block the gateway{' '}
              <span className="font-medium">{blockGatewayConfirm?.name}</span>? This will immediately
              disconnect all {blockGatewayConfirm?.agent_count} agent
              {blockGatewayConfirm?.agent_count !== 1 ? 's' : ''} and prevent the gateway from
              reconnecting.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setBlockGatewayConfirm(null)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleBlockGateway} disabled={blockGateway.isPending}>
              {blockGateway.isPending ? 'Blocking...' : 'Block Gateway'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={!!blockAgentConfirm} onOpenChange={(open) => !open && setBlockAgentConfirm(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <ShieldAlert className="h-5 w-5 text-destructive" />
              Block Agent
            </DialogTitle>
            <DialogDescription>
              Are you sure you want to block the agent{' '}
              <span className="font-medium">{blockAgentConfirm?.hostname}</span>? It will be immediately
              disconnected and unable to reconnect until unblocked.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setBlockAgentConfirm(null)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleBlockAgent} disabled={blockAgent.isPending}>
              {blockAgent.isPending ? 'Blocking...' : 'Block Agent'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={!!assignSiteGateway} onOpenChange={(open) => !open && setAssignSiteGateway(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <MapPin className="h-5 w-5" />
              Assign Gateway to Site
            </DialogTitle>
            <DialogDescription>
              Select a site for the gateway{' '}
              <span className="font-medium">{assignSiteGateway?.name}</span>.
              Gateways in the same site share failover responsibility.
            </DialogDescription>
          </DialogHeader>
          <div className="py-4">
            <Select value={selectedSiteId} onValueChange={setSelectedSiteId}>
              <SelectTrigger>
                <SelectValue placeholder="Select a site..." />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="unassigned">
                  <span className="flex items-center gap-2 text-muted-foreground">
                    <MapPin className="h-4 w-4" />
                    Unassigned
                  </span>
                </SelectItem>
                {allSites?.filter((s) => s.is_active).map((site) => (
                  <SelectItem key={site.id} value={site.id}>
                    <span className="flex items-center gap-2">
                      <MapPin className="h-4 w-4" />
                      {site.name}
                      <span className="text-muted-foreground">({site.code})</span>
                    </span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {(!allSites || allSites.filter((s) => s.is_active).length === 0) && (
              <p className="text-sm text-muted-foreground mt-2">
                No sites available. Create a site first in the Sites page.
              </p>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setAssignSiteGateway(null)}>
              Cancel
            </Button>
            <Button onClick={handleAssignSite} disabled={updateGateway.isPending}>
              {updateGateway.isPending ? 'Saving...' : 'Save'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {logsGateway && (
        <LogViewerModal
          gatewayId={logsGateway.id}
          sourceName={logsGateway.name}
          sourceType="gateway"
          open={!!logsGateway}
          onClose={() => setLogsGateway(null)}
        />
      )}
    </div>
  );
}
