import { useAgents, type Agent } from '@/api/reports';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from '@/components/ui/table';
import { Server, Wifi, WifiOff } from 'lucide-react';

export function AgentsPage() {
  const { data: agents, isLoading } = useAgents();

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin h-8 w-8 border-2 border-primary border-t-transparent rounded-full" />
      </div>
    );
  }

  const agentList: Agent[] = agents || [];

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Agents</h1>

      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Agent</TableHead>
                <TableHead>Hostname</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Version</TableHead>
                <TableHead>Last Heartbeat</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {!agentList.length ? (
                <TableRow>
                  <TableCell colSpan={5} className="text-center text-muted-foreground py-8">
                    No agents registered
                  </TableCell>
                </TableRow>
              ) : (
                agentList.map((agent) => (
                  <TableRow key={agent.id}>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <Server className="h-4 w-4 text-muted-foreground" />
                        <span className="font-medium font-mono text-xs">{agent.id?.slice(0, 8)}</span>
                      </div>
                    </TableCell>
                    <TableCell>{agent.hostname || '-'}</TableCell>
                    <TableCell>
                      {agent.status === 'connected' ? (
                        <Badge variant="running" className="gap-1">
                          <Wifi className="h-3 w-3" /> Connected
                        </Badge>
                      ) : (
                        <Badge variant="stopped" className="gap-1">
                          <WifiOff className="h-3 w-3" /> Disconnected
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell className="text-muted-foreground">{agent.version || '-'}</TableCell>
                    <TableCell className="text-muted-foreground text-sm">
                      {agent.last_heartbeat ? new Date(agent.last_heartbeat).toLocaleString() : '-'}
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}
