import { useApps } from '@/api/apps';
import { useAuditLog } from '@/api/reports';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { BarChart3, FileText, Shield } from 'lucide-react';

export function ReportsPage() {
  const { data: apps } = useApps();
  const { data: auditEntries, isLoading: auditLoading } = useAuditLog({ limit: 50 });

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Reports</h1>

      <Tabs defaultValue="audit">
        <TabsList>
          <TabsTrigger value="audit">Audit Trail</TabsTrigger>
          <TabsTrigger value="availability">Availability</TabsTrigger>
          <TabsTrigger value="compliance">Compliance</TabsTrigger>
        </TabsList>

        <TabsContent value="audit">
          <Card>
            <CardHeader>
              <CardTitle className="text-lg flex items-center gap-2">
                <FileText className="h-5 w-5" /> Audit Log
              </CardTitle>
            </CardHeader>
            <CardContent>
              <ScrollArea className="h-[500px]">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Time</TableHead>
                      <TableHead>User</TableHead>
                      <TableHead>Action</TableHead>
                      <TableHead>Target</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {auditLoading ? (
                      <TableRow>
                        <TableCell colSpan={4} className="text-center py-8">Loading...</TableCell>
                      </TableRow>
                    ) : !auditEntries?.length ? (
                      <TableRow>
                        <TableCell colSpan={4} className="text-center text-muted-foreground py-8">
                          No audit entries
                        </TableCell>
                      </TableRow>
                    ) : (
                      auditEntries.map((entry) => (
                        <TableRow key={entry.id}>
                          <TableCell className="text-sm text-muted-foreground whitespace-nowrap">
                            {new Date(entry.created_at).toLocaleString()}
                          </TableCell>
                          <TableCell className="text-sm">{entry.user_email}</TableCell>
                          <TableCell>
                            <Badge variant="outline">{entry.action}</Badge>
                          </TableCell>
                          <TableCell className="text-sm text-muted-foreground">
                            {entry.target_type}/{entry.target_id?.slice(0, 8)}
                          </TableCell>
                        </TableRow>
                      ))
                    )}
                  </TableBody>
                </Table>
              </ScrollArea>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="availability">
          <Card>
            <CardHeader>
              <CardTitle className="text-lg flex items-center gap-2">
                <BarChart3 className="h-5 w-5" /> Availability Reports
              </CardTitle>
            </CardHeader>
            <CardContent>
              {!apps?.length ? (
                <p className="text-sm text-muted-foreground text-center py-8">No applications to report on</p>
              ) : (
                <div className="space-y-3">
                  {apps.map((app) => (
                    <div key={app.id} className="flex items-center justify-between p-3 rounded-lg border border-border">
                      <div>
                        <p className="font-medium text-sm">{app.name}</p>
                        <p className="text-xs text-muted-foreground">{app.component_count} components</p>
                      </div>
                      <Badge variant="running">Reporting Available</Badge>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="compliance">
          <Card>
            <CardHeader>
              <CardTitle className="text-lg flex items-center gap-2">
                <Shield className="h-5 w-5" /> DORA Compliance
              </CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-muted-foreground text-center py-8">
                Select an application to view DORA compliance metrics.
              </p>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
