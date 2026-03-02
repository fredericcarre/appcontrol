import { useState } from 'react';
import {
  useGatewayZones,
  useGatewayAgents,
  useSuspendGateway,
  useActivateGateway,
  useSetGatewayPrimary,
  useDeleteGateway,
  type Gateway,
  type ZoneSummary,
} from '@/api/gateways';
import { useAuthStore } from '@/stores/auth';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
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
  Network,
  Server,
  Wifi,
  WifiOff,
  ChevronRight,
  ChevronDown,
  MoreHorizontal,
  Pause,
  Play,
  Trash2,
  ShieldAlert,
  Star,
  AlertTriangle,
  Clock,
} from 'lucide-react';

function formatTimeAgo(dateStr: string | null): string {
  if (!dateStr) return 'Never';
  const date = new Date(dateStr);
  const seconds = Math.floor((Date.now() - date.getTime()) / 1000);
  if (seconds < 60) return `${seconds}s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}

function getRoleBadge(gateway: Gateway) {
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

interface GatewayItemProps {
  gateway: Gateway;
  isAdmin: boolean;
  onSuspend: (gateway: Gateway) => void;
  onActivate: (gateway: Gateway) => void;
  onSetPrimary: (gateway: Gateway) => void;
  onDelete: (gateway: Gateway) => void;
}

function GatewayItem({
  gateway,
  isAdmin,
  onSuspend,
  onActivate,
  onSetPrimary,
  onDelete,
}: GatewayItemProps) {
  const [expanded, setExpanded] = useState(false);
  const { data: agents, isLoading } = useGatewayAgents(expanded ? gateway.id : '');

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
        <div className="flex items-center gap-2 ml-auto">
          {getRoleBadge(gateway)}
          {getConnectionBadge(gateway)}
          <span className="text-xs text-muted-foreground flex items-center gap-1">
            <Clock className="h-3 w-3" />
            {formatTimeAgo(gateway.last_heartbeat_at)}
          </span>
          <span className="text-sm text-muted-foreground">
            {gateway.agent_count} agent{gateway.agent_count !== 1 ? 's' : ''}
          </span>
          {isAdmin && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild onClick={(e) => e.stopPropagation()}>
                <Button variant="ghost" size="icon" className="h-8 w-8">
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {!gateway.is_primary && (
                  <DropdownMenuItem onClick={() => onSetPrimary(gateway)}>
                    <Star className="h-4 w-4 mr-2" />
                    Set as Primary
                  </DropdownMenuItem>
                )}
                {gateway.status === 'suspended' ? (
                  <DropdownMenuItem onClick={() => onActivate(gateway)}>
                    <Play className="h-4 w-4 mr-2" />
                    Activate
                  </DropdownMenuItem>
                ) : (
                  <DropdownMenuItem onClick={() => onSuspend(gateway)}>
                    <Pause className="h-4 w-4 mr-2" />
                    Suspend
                  </DropdownMenuItem>
                )}
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => onDelete(gateway)} className="text-destructive">
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
            <Server className="h-4 w-4" /> Connected Agents
          </h4>
          {isLoading ? (
            <div className="text-sm text-muted-foreground">Loading agents...</div>
          ) : !agents?.length ? (
            <div className="text-sm text-muted-foreground">No agents connected</div>
          ) : (
            <div className="space-y-1">
              {agents.map((agent) => (
                <div key={agent.id} className="flex items-center gap-3 text-sm py-1">
                  <Server className="h-3 w-3 text-muted-foreground" />
                  <span className="font-mono text-xs">{agent.id.slice(0, 8)}</span>
                  <span>{agent.hostname}</span>
                  {agent.is_active ? (
                    <Badge variant="default" className="text-xs bg-green-600">
                      Active
                    </Badge>
                  ) : (
                    <Badge variant="secondary" className="text-xs">
                      Inactive
                    </Badge>
                  )}
                  {agent.last_heartbeat_at && (
                    <span className="text-xs text-muted-foreground">
                      Last: {formatTimeAgo(agent.last_heartbeat_at)}
                    </span>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

interface ZoneCardProps {
  zone: ZoneSummary;
  isAdmin: boolean;
  onSuspend: (gateway: Gateway) => void;
  onActivate: (gateway: Gateway) => void;
  onSetPrimary: (gateway: Gateway) => void;
  onDelete: (gateway: Gateway) => void;
}

function ZoneCard({ zone, isAdmin, onSuspend, onActivate, onSetPrimary, onDelete }: ZoneCardProps) {
  const [collapsed, setCollapsed] = useState(false);
  const connectedCount = zone.gateways.filter((g) => g.connected).length;

  return (
    <Card className={zone.failover_active ? 'border-orange-300 dark:border-orange-800' : ''}>
      <CardHeader
        className="cursor-pointer hover:bg-muted/50 transition-colors"
        onClick={() => setCollapsed(!collapsed)}
      >
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Button variant="ghost" size="icon" className="h-6 w-6">
              {collapsed ? <ChevronRight className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
            </Button>
            <CardTitle className="text-lg">{zone.zone}</CardTitle>
            <Badge variant="secondary">
              {zone.gateway_count} gateway{zone.gateway_count !== 1 ? 's' : ''}
            </Badge>
          </div>
          <div className="flex items-center gap-2">
            {zone.failover_active && (
              <Badge variant="destructive" className="gap-1">
                <AlertTriangle className="h-3 w-3" /> Failover Active
              </Badge>
            )}
            <span className="text-sm text-muted-foreground">
              {connectedCount}/{zone.gateway_count} online
            </span>
          </div>
        </div>
      </CardHeader>
      {!collapsed && (
        <CardContent className="pt-0">
          {zone.gateways.map((gateway) => (
            <GatewayItem
              key={gateway.id}
              gateway={gateway}
              isAdmin={isAdmin}
              onSuspend={onSuspend}
              onActivate={onActivate}
              onSetPrimary={onSetPrimary}
              onDelete={onDelete}
            />
          ))}
        </CardContent>
      )}
    </Card>
  );
}

export function GatewaysPage() {
  const user = useAuthStore((s) => s.user);
  const isAdmin = user?.role === 'admin';
  const { data: zones, isLoading } = useGatewayZones();
  const suspendGateway = useSuspendGateway();
  const activateGateway = useActivateGateway();
  const setGatewayPrimary = useSetGatewayPrimary();
  const deleteGateway = useDeleteGateway();

  const [deleteConfirm, setDeleteConfirm] = useState<Gateway | null>(null);

  const handleSuspend = async (gateway: Gateway) => {
    await suspendGateway.mutateAsync(gateway.id);
  };

  const handleActivate = async (gateway: Gateway) => {
    await activateGateway.mutateAsync(gateway.id);
  };

  const handleSetPrimary = async (gateway: Gateway) => {
    await setGatewayPrimary.mutateAsync(gateway.id);
  };

  const handleDelete = async () => {
    if (!deleteConfirm) return;
    await deleteGateway.mutateAsync(deleteConfirm.id);
    setDeleteConfirm(null);
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin h-8 w-8 border-2 border-primary border-t-transparent rounded-full" />
      </div>
    );
  }

  const zoneList = zones || [];
  const totalGateways = zoneList.reduce((sum, z) => sum + z.gateway_count, 0);
  const totalConnected = zoneList.reduce(
    (sum, z) => sum + z.gateways.filter((g) => g.connected).length,
    0
  );
  const failoverZones = zoneList.filter((z) => z.failover_active);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Gateways</h1>
        <div className="flex items-center gap-4">
          {failoverZones.length > 0 && (
            <Badge variant="destructive" className="gap-1">
              <AlertTriangle className="h-3 w-3" />
              {failoverZones.length} zone{failoverZones.length !== 1 ? 's' : ''} in failover
            </Badge>
          )}
          <span className="text-sm text-muted-foreground">
            {totalConnected}/{totalGateways} gateways online
          </span>
        </div>
      </div>

      {!zoneList.length ? (
        <Card>
          <CardContent className="flex flex-col items-center justify-center py-12 text-center">
            <Network className="h-12 w-12 text-muted-foreground mb-4" />
            <h3 className="font-medium text-lg mb-2">No Gateways Registered</h3>
            <p className="text-muted-foreground max-w-md">
              Start a gateway to see it here. Gateways relay agent connections to the backend and
              handle mTLS authentication.
            </p>
          </CardContent>
        </Card>
      ) : (
        <div className="space-y-4">
          {zoneList.map((zone) => (
            <ZoneCard
              key={zone.zone}
              zone={zone}
              isAdmin={isAdmin}
              onSuspend={handleSuspend}
              onActivate={handleActivate}
              onSetPrimary={handleSetPrimary}
              onDelete={setDeleteConfirm}
            />
          ))}
        </div>
      )}

      <Card>
        <CardHeader>
          <CardTitle className="text-lg">About Zones & Failover</CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground space-y-2">
          <p>
            <strong>Zones</strong> group gateways that can serve the same agents. Each zone has one{' '}
            <strong>primary</strong> gateway and zero or more <strong>standby</strong> gateways.
          </p>
          <p>
            When the primary gateway goes offline, the standby with the lowest priority number takes
            over automatically (<strong>failover</strong>). Agents can enroll via any gateway in
            their authorized zone.
          </p>
        </CardContent>
      </Card>

      {/* Delete Confirmation Dialog */}
      <Dialog open={!!deleteConfirm} onOpenChange={(open) => !open && setDeleteConfirm(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <ShieldAlert className="h-5 w-5 text-destructive" />
              Delete Gateway
            </DialogTitle>
            <DialogDescription>
              Are you sure you want to delete the gateway{' '}
              <span className="font-medium">{deleteConfirm?.name}</span> in zone{' '}
              <span className="font-medium">{deleteConfirm?.zone}</span>? This will disconnect all
              agents connected through this gateway.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteConfirm(null)}>
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleDelete}
              disabled={deleteGateway.isPending}
            >
              {deleteGateway.isPending ? 'Deleting...' : 'Delete'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
