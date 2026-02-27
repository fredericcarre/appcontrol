import { useState } from 'react';
import {
  useGateways,
  useGatewayAgents,
  useSuspendGateway,
  useActivateGateway,
  useDeleteGateway,
  type Gateway,
} from '@/api/gateways';
import { useAuthStore } from '@/stores/auth';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
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
} from 'lucide-react';

function getStatusBadge(gateway: Gateway) {
  if (gateway.status === 'suspended') {
    return <Badge variant="secondary">Suspended</Badge>;
  }
  if (gateway.connected) {
    return (
      <Badge variant="default" className="gap-1 bg-green-600 hover:bg-green-700">
        <Wifi className="h-3 w-3" /> Connected
      </Badge>
    );
  }
  return (
    <Badge variant="outline" className="gap-1">
      <WifiOff className="h-3 w-3" /> Disconnected
    </Badge>
  );
}

interface GatewayRowProps {
  gateway: Gateway;
  isAdmin: boolean;
  onSuspend: (gateway: Gateway) => void;
  onActivate: (gateway: Gateway) => void;
  onDelete: (gateway: Gateway) => void;
}

function GatewayRow({ gateway, isAdmin, onSuspend, onActivate, onDelete }: GatewayRowProps) {
  const [expanded, setExpanded] = useState(false);
  const { data: agents, isLoading } = useGatewayAgents(expanded ? gateway.id : '');

  return (
    <>
      <TableRow
        className="cursor-pointer hover:bg-muted/50"
        onClick={() => setExpanded(!expanded)}
      >
        <TableCell>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="icon" className="h-6 w-6">
              {expanded ? (
                <ChevronDown className="h-4 w-4" />
              ) : (
                <ChevronRight className="h-4 w-4" />
              )}
            </Button>
            <Network className="h-4 w-4 text-muted-foreground" />
            <span className="font-medium">{gateway.name}</span>
          </div>
        </TableCell>
        <TableCell>
          <Badge variant="secondary">{gateway.zone}</Badge>
        </TableCell>
        <TableCell>{getStatusBadge(gateway)}</TableCell>
        <TableCell className="text-right">
          <span className="text-sm">{gateway.agent_count}</span>
        </TableCell>
        {isAdmin && (
          <TableCell>
            <DropdownMenu>
              <DropdownMenuTrigger asChild onClick={(e) => e.stopPropagation()}>
                <Button variant="ghost" size="icon" className="h-8 w-8">
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
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
                <DropdownMenuItem onClick={() => onDelete(gateway)} className="text-destructive">
                  <Trash2 className="h-4 w-4 mr-2" />
                  Delete
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </TableCell>
        )}
      </TableRow>
      {expanded && (
        <TableRow>
          <TableCell colSpan={isAdmin ? 5 : 4} className="bg-muted/30 p-0">
            <div className="p-4 pl-12">
              <h4 className="text-sm font-medium mb-2 flex items-center gap-2">
                <Server className="h-4 w-4" /> Connected Agents
              </h4>
              {isLoading ? (
                <div className="text-sm text-muted-foreground">Loading agents...</div>
              ) : !agents?.length ? (
                <div className="text-sm text-muted-foreground">
                  No agents connected to this gateway
                </div>
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
                          Last seen: {new Date(agent.last_heartbeat_at).toLocaleString()}
                        </span>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          </TableCell>
        </TableRow>
      )}
    </>
  );
}

export function GatewaysPage() {
  const user = useAuthStore((s) => s.user);
  const isAdmin = user?.role === 'admin';
  const { data: gateways, isLoading } = useGateways();
  const suspendGateway = useSuspendGateway();
  const activateGateway = useActivateGateway();
  const deleteGateway = useDeleteGateway();

  const [deleteConfirm, setDeleteConfirm] = useState<Gateway | null>(null);

  const handleSuspend = async (gateway: Gateway) => {
    await suspendGateway.mutateAsync(gateway.id);
  };

  const handleActivate = async (gateway: Gateway) => {
    await activateGateway.mutateAsync(gateway.id);
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

  const gatewayList = gateways || [];
  const connectedCount = gatewayList.filter((g) => g.connected).length;

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Gateways</h1>
        <div className="text-sm text-muted-foreground">
          {connectedCount} of {gatewayList.length} gateway{gatewayList.length !== 1 ? 's' : ''}{' '}
          connected
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-lg flex items-center gap-2">
            <Network className="h-5 w-5" />
            Gateways
          </CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Gateway</TableHead>
                <TableHead>Zone</TableHead>
                <TableHead>Status</TableHead>
                <TableHead className="text-right">Agents</TableHead>
                {isAdmin && <TableHead className="w-[50px]"></TableHead>}
              </TableRow>
            </TableHeader>
            <TableBody>
              {!gatewayList.length ? (
                <TableRow>
                  <TableCell
                    colSpan={isAdmin ? 5 : 4}
                    className="text-center text-muted-foreground py-8"
                  >
                    No gateways registered. Start a gateway to see it here.
                  </TableCell>
                </TableRow>
              ) : (
                gatewayList.map((gateway) => (
                  <GatewayRow
                    key={gateway.id}
                    gateway={gateway}
                    isAdmin={isAdmin}
                    onSuspend={handleSuspend}
                    onActivate={handleActivate}
                    onDelete={setDeleteConfirm}
                  />
                ))
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-lg">About Gateways</CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground space-y-2">
          <p>
            Gateways are relay servers that connect agents to the AppControl backend. They handle
            mTLS authentication and route commands to the appropriate agents.
          </p>
          <p>
            Each gateway serves a zone (e.g., "production", "dmz") and can have multiple agents
            connected. Click on a gateway row to see its connected agents.
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
              <span className="font-medium">{deleteConfirm?.name}</span> ({deleteConfirm?.zone})?
              This will disconnect all agents connected through this gateway.
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
