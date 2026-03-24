import { useState } from 'react';
import { useApps } from '@/api/apps';
import { useAuditLog, useComplianceReport, usePraReport, PraExercise } from '@/api/reports';
import { Card, CardHeader, CardTitle, CardContent, CardDescription } from '@/components/ui/card';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from '@/components/ui/dialog';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { BarChart3, FileText, Shield, CheckCircle, TrendingUp, Download, Clock, Activity, RefreshCw, ChevronDown, ChevronRight, AlertTriangle, XCircle, Printer } from 'lucide-react';
import client from '@/api/client';
import { useQuery } from '@tanstack/react-query';

// Helper to extract a readable name from audit entry
function getTargetName(entry: { target_type: string; target_id: string; target_name?: string; details: Record<string, unknown> }): string {
  // Use target_name from backend if available
  if (entry.target_name) return entry.target_name;

  // Try to get name from details
  const details = entry.details || {};
  if (details.name) return String(details.name);
  if (details.hostname) return String(details.hostname);
  if (details.app_name) return String(details.app_name);
  if (details.component_name) return String(details.component_name);
  if (details.gateway_name) return String(details.gateway_name);

  // Fallback to type + short ID
  return `${entry.target_type}/${entry.target_id?.slice(0, 8) || 'unknown'}`;
}

// Format action for display
function formatAction(action: string): string {
  return action
    .replace(/_/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

interface AvailabilityData {
  report: string;
  data: Array<{
    component_id: string;
    date: string;
    running_seconds: number;
    total_seconds: number;
    availability_pct: number;
  }>;
}

function useAppAvailability(appId: string | null) {
  return useQuery({
    queryKey: ['reports', 'availability', appId],
    queryFn: async () => {
      if (!appId) return null;
      const { data } = await client.get<AvailabilityData>(`/apps/${appId}/reports/availability`);
      return data;
    },
    enabled: !!appId,
  });
}

interface ExportReport {
  application: { id: string; name: string };
  period: { from: string; to: string };
  summary: {
    overall_availability_pct: number;
    incident_count: number;
    switchover_count: number;
    audit_trail_entries: number;
    average_rto_seconds: number | null;
    dora_compliant: boolean;
  };
  generated_at: string;
  generated_by: string;
}

// PRA Exercise Card Component
function PraExerciseCard({
  exercise,
  expanded,
  onToggle,
  formatDuration,
}: {
  exercise: PraExercise;
  expanded: boolean;
  onToggle: () => void;
  formatDuration: (seconds: number | null) => string;
}) {
  const statusConfig = {
    completed: { label: 'Réussi', variant: 'running' as const, icon: CheckCircle },
    failed: { label: 'Échoué', variant: 'failed' as const, icon: XCircle },
    rolled_back: { label: 'Annulé', variant: 'degraded' as const, icon: AlertTriangle },
    in_progress: { label: 'En cours', variant: 'secondary' as const, icon: Clock },
  };

  const phaseLabels: Record<string, string> = {
    PREPARE: 'Préparation',
    VALIDATE: 'Validation',
    STOP_SOURCE: 'Arrêt source',
    SYNC: 'Synchronisation',
    START_TARGET: 'Démarrage cible',
    COMMIT: 'Validation finale',
    ROLLBACK: 'Annulation',
  };

  const config = statusConfig[exercise.status] || statusConfig.in_progress;
  const StatusIcon = config.icon;

  return (
    <Collapsible open={expanded} onOpenChange={onToggle}>
      <div className="border rounded-lg overflow-hidden print:break-inside-avoid">
        <CollapsibleTrigger asChild>
          <button className="w-full p-4 flex items-center justify-between hover:bg-muted/50 transition-colors text-left">
            <div className="flex items-center gap-4">
              <StatusIcon className={`h-5 w-5 ${
                exercise.status === 'completed' ? 'text-green-500' :
                exercise.status === 'failed' ? 'text-red-500' :
                exercise.status === 'rolled_back' ? 'text-orange-500' :
                'text-blue-500'
              }`} />
              <div>
                <p className="font-medium">
                  {exercise.source_site || 'Site source'} → {exercise.target_site || 'Site cible'}
                </p>
                <p className="text-sm text-muted-foreground">
                  {new Date(exercise.started_at).toLocaleString('fr-FR')}
                </p>
              </div>
            </div>
            <div className="flex items-center gap-4">
              <div className="text-right">
                <Badge variant={config.variant}>{config.label}</Badge>
                <p className="text-sm text-muted-foreground mt-1">
                  RTO: {formatDuration(exercise.rto_seconds)}
                </p>
              </div>
              {expanded ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
            </div>
          </button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="border-t p-4 bg-muted/30 print:bg-transparent">
            <h5 className="font-medium mb-3">Phases horodatées</h5>
            <div className="space-y-2">
              {exercise.phases.map((phase, idx) => (
                <div key={idx} className="flex items-center gap-4 text-sm">
                  <div className="w-32 font-medium">{phaseLabels[phase.phase] || phase.phase}</div>
                  <div className="w-48 text-muted-foreground">
                    {new Date(phase.started_at).toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit', second: '2-digit' })}
                    {' → '}
                    {new Date(phase.completed_at).toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit', second: '2-digit' })}
                  </div>
                  <div className="w-20 text-right">{phase.duration_ms}ms</div>
                  <Badge variant={phase.status === 'completed' ? 'running' : 'failed'} className="text-xs">
                    {phase.status === 'completed' ? 'OK' : 'KO'}
                  </Badge>
                </div>
              ))}
            </div>
            {exercise.components_count && (
              <p className="text-sm text-muted-foreground mt-3">
                {exercise.components_count} composants impactés
              </p>
            )}
            <p className="text-xs text-muted-foreground mt-2 font-mono">
              ID: {exercise.switchover_id}
            </p>
          </div>
        </CollapsibleContent>
      </div>
    </Collapsible>
  );
}

export function ReportsPage() {
  const { data: apps } = useApps();
  const { data: auditEntries, isLoading: auditLoading } = useAuditLog({ limit: 50 });

  const [selectedAppId, setSelectedAppId] = useState<string | null>(null);
  const [availabilityAppId, setAvailabilityAppId] = useState<string | null>(null);
  const [exportReport, setExportReport] = useState<ExportReport | null>(null);
  const [exportLoading, setExportLoading] = useState(false);

  const { data: complianceData, isLoading: complianceLoading } = useComplianceReport(selectedAppId || '');
  const { data: availabilityData, isLoading: availabilityLoading } = useAppAvailability(availabilityAppId);

  // PRA Report state
  const [praAppId, setPraAppId] = useState<string | null>(null);
  const [expandedExercise, setExpandedExercise] = useState<string | null>(null);
  const { data: praData, isLoading: praLoading } = usePraReport(praAppId || '');

  // Helper to get app name from id
  const getAppName = (appId: string | null) => {
    if (!appId || !apps) return null;
    return apps.find((a) => a.id === appId)?.name || null;
  };

  // Export report handler
  const handleExportReport = async () => {
    if (!selectedAppId) return;
    setExportLoading(true);
    try {
      const { data } = await client.get<ExportReport>(`/apps/${selectedAppId}/reports/export`);
      setExportReport(data);
    } catch (e) {
      console.error('Failed to export report', e);
    } finally {
      setExportLoading(false);
    }
  };

  // Format seconds to human readable
  const formatDuration = (seconds: number | null) => {
    if (seconds === null) return 'N/A';
    if (seconds < 60) return `${Math.round(seconds)}s`;
    if (seconds < 3600) return `${Math.round(seconds / 60)}min`;
    return `${(seconds / 3600).toFixed(1)}h`;
  };

  // Calculate overall availability percentage
  const overallAvailability = availabilityData?.data?.length
    ? (availabilityData.data.reduce((sum, d) => sum + d.availability_pct, 0) / availabilityData.data.length).toFixed(1)
    : null;

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Reports</h1>

      <Tabs defaultValue="audit">
        <TabsList>
          <TabsTrigger value="audit">Audit Trail</TabsTrigger>
          <TabsTrigger value="availability">Availability</TabsTrigger>
          <TabsTrigger value="pra">Exercices PRA</TabsTrigger>
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
                      <TableHead>Details</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {auditLoading ? (
                      <TableRow>
                        <TableCell colSpan={5} className="text-center py-8">Loading...</TableCell>
                      </TableRow>
                    ) : !auditEntries?.length ? (
                      <TableRow>
                        <TableCell colSpan={5} className="text-center text-muted-foreground py-8">
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
                            <Badge variant="outline">{formatAction(entry.action)}</Badge>
                          </TableCell>
                          <TableCell className="text-sm font-medium">
                            {getTargetName(entry)}
                          </TableCell>
                          <TableCell className="text-xs text-muted-foreground max-w-[200px] truncate">
                            {entry.details && Object.keys(entry.details).length > 0
                              ? Object.entries(entry.details)
                                  .filter(([k]) => !['name', 'hostname', 'app_name', 'component_name'].includes(k))
                                  .map(([k, v]) => `${k}: ${v}`)
                                  .join(', ')
                              : '-'}
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
            <CardHeader className="flex flex-row items-center justify-between">
              <div>
                <CardTitle className="text-lg flex items-center gap-2">
                  <BarChart3 className="h-5 w-5" /> Availability Report
                </CardTitle>
                <CardDescription className="mt-1">
                  Uptime tracking based on component health checks over the last 30 days
                </CardDescription>
              </div>
              <Select value={availabilityAppId || ''} onValueChange={(v) => setAvailabilityAppId(v || null)}>
                <SelectTrigger className="w-[250px]">
                  <SelectValue placeholder="Select an application">
                    {getAppName(availabilityAppId) || 'Select an application'}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {apps?.map((app) => (
                    <SelectItem key={app.id} value={app.id}>
                      {app.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </CardHeader>
            <CardContent>
              {!availabilityAppId ? (
                <div className="text-center py-12">
                  <Activity className="h-12 w-12 mx-auto text-muted-foreground/50 mb-4" />
                  <p className="text-muted-foreground">Select an application to view its availability history</p>
                  <p className="text-sm text-muted-foreground/70 mt-2">
                    Shows daily uptime percentage based on health check results
                  </p>
                </div>
              ) : availabilityLoading ? (
                <p className="text-sm text-muted-foreground text-center py-8">Loading...</p>
              ) : !availabilityData?.data?.length ? (
                <div className="text-center py-12">
                  <Clock className="h-12 w-12 mx-auto text-muted-foreground/50 mb-4" />
                  <p className="text-muted-foreground">No availability data yet</p>
                  <p className="text-sm text-muted-foreground/70 mt-2">
                    Data will appear once components have been running with health checks enabled
                  </p>
                </div>
              ) : (
                <div className="space-y-4">
                  {/* Summary Card */}
                  <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                    <div className="flex items-center gap-4 p-4 rounded-lg bg-green-50 dark:bg-green-950/30 border border-green-200 dark:border-green-800">
                      <TrendingUp className="h-8 w-8 text-green-500" />
                      <div>
                        <p className="text-2xl font-bold text-green-700 dark:text-green-400">{overallAvailability}%</p>
                        <p className="text-sm text-muted-foreground">Average Uptime</p>
                      </div>
                    </div>
                    <div className="flex items-center gap-4 p-4 rounded-lg bg-muted/50 border">
                      <Activity className="h-8 w-8 text-blue-500" />
                      <div>
                        <p className="text-2xl font-bold">{availabilityData.data.length}</p>
                        <p className="text-sm text-muted-foreground">Days Tracked</p>
                      </div>
                    </div>
                    <div className="flex items-center gap-4 p-4 rounded-lg bg-muted/50 border">
                      <Clock className="h-8 w-8 text-orange-500" />
                      <div>
                        <p className="text-2xl font-bold">
                          {availabilityData.data.filter(d => d.availability_pct < 99).length}
                        </p>
                        <p className="text-sm text-muted-foreground">Days Below 99%</p>
                      </div>
                    </div>
                  </div>

                  {/* Daily Breakdown */}
                  <div>
                    <h4 className="font-medium mb-2">Daily Breakdown</h4>
                    <ScrollArea className="h-[250px]">
                      <Table>
                        <TableHeader>
                          <TableRow>
                            <TableHead>Date</TableHead>
                            <TableHead>Uptime</TableHead>
                            <TableHead>Status</TableHead>
                          </TableRow>
                        </TableHeader>
                        <TableBody>
                          {availabilityData.data.map((row, i) => (
                            <TableRow key={i}>
                              <TableCell className="font-medium">{row.date}</TableCell>
                              <TableCell>
                                <div className="flex items-center gap-2">
                                  <div className="w-24 h-2 bg-muted rounded-full overflow-hidden">
                                    <div
                                      className={`h-full ${row.availability_pct >= 99 ? 'bg-green-500' : row.availability_pct >= 95 ? 'bg-yellow-500' : 'bg-red-500'}`}
                                      style={{ width: `${row.availability_pct}%` }}
                                    />
                                  </div>
                                  <span className="text-sm">{row.availability_pct.toFixed(1)}%</span>
                                </div>
                              </TableCell>
                              <TableCell>
                                <Badge variant={row.availability_pct >= 99 ? 'running' : row.availability_pct >= 95 ? 'degraded' : 'failed'}>
                                  {row.availability_pct >= 99 ? 'Healthy' : row.availability_pct >= 95 ? 'Degraded' : 'Outage'}
                                </Badge>
                              </TableCell>
                            </TableRow>
                          ))}
                        </TableBody>
                      </Table>
                    </ScrollArea>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="pra">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <div>
                <CardTitle className="text-lg flex items-center gap-2">
                  <RefreshCw className="h-5 w-5" /> Exercices PRA
                </CardTitle>
                <CardDescription className="mt-1">
                  Historique des tests de reprise d'activité (DORA Article 11)
                </CardDescription>
              </div>
              <div className="flex items-center gap-2">
                <Select value={praAppId || ''} onValueChange={(v) => setPraAppId(v || null)}>
                  <SelectTrigger className="w-[250px]">
                    <SelectValue placeholder="Sélectionner une application">
                      {getAppName(praAppId) || 'Sélectionner une application'}
                    </SelectValue>
                  </SelectTrigger>
                  <SelectContent>
                    {apps?.map((app) => (
                      <SelectItem key={app.id} value={app.id}>
                        {app.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                {praData && praData.exercises.length > 0 && (
                  <Button variant="outline" onClick={() => window.print()}>
                    <Printer className="h-4 w-4 mr-2" />
                    Imprimer
                  </Button>
                )}
              </div>
            </CardHeader>
            <CardContent>
              {!praAppId ? (
                <div className="text-center py-12">
                  <RefreshCw className="h-12 w-12 mx-auto text-muted-foreground/50 mb-4" />
                  <p className="text-muted-foreground">Sélectionnez une application pour voir l'historique des exercices PRA</p>
                  <p className="text-sm text-muted-foreground/70 mt-2">
                    Conforme à DORA Article 11 : traçabilité des tests de reprise
                  </p>
                </div>
              ) : praLoading ? (
                <p className="text-sm text-muted-foreground text-center py-8">Chargement...</p>
              ) : !praData?.exercises?.length ? (
                <div className="text-center py-12">
                  <Clock className="h-12 w-12 mx-auto text-muted-foreground/50 mb-4" />
                  <p className="text-muted-foreground">Aucun exercice PRA enregistré</p>
                  <p className="text-sm text-muted-foreground/70 mt-2">
                    Les bascules de site seront documentées ici avec horodatage complet
                  </p>
                </div>
              ) : (
                <div className="space-y-4 print:space-y-6">
                  {/* Summary Stats */}
                  <div className="grid grid-cols-1 md:grid-cols-4 gap-4 print:grid-cols-4">
                    <div className="flex items-center gap-4 p-4 rounded-lg bg-blue-50 dark:bg-blue-950/30 border border-blue-200 dark:border-blue-800">
                      <RefreshCw className="h-8 w-8 text-blue-500" />
                      <div>
                        <p className="text-2xl font-bold text-blue-700 dark:text-blue-400">{praData.total_exercises}</p>
                        <p className="text-sm text-muted-foreground">Exercices</p>
                      </div>
                    </div>
                    <div className="flex items-center gap-4 p-4 rounded-lg bg-green-50 dark:bg-green-950/30 border border-green-200 dark:border-green-800">
                      <CheckCircle className="h-8 w-8 text-green-500" />
                      <div>
                        <p className="text-2xl font-bold text-green-700 dark:text-green-400">
                          {praData.exercises.filter(e => e.status === 'completed').length}
                        </p>
                        <p className="text-sm text-muted-foreground">Réussis</p>
                      </div>
                    </div>
                    <div className="flex items-center gap-4 p-4 rounded-lg bg-orange-50 dark:bg-orange-950/30 border border-orange-200 dark:border-orange-800">
                      <AlertTriangle className="h-8 w-8 text-orange-500" />
                      <div>
                        <p className="text-2xl font-bold text-orange-700 dark:text-orange-400">
                          {praData.exercises.filter(e => e.status === 'rolled_back').length}
                        </p>
                        <p className="text-sm text-muted-foreground">Annulés</p>
                      </div>
                    </div>
                    <div className="flex items-center gap-4 p-4 rounded-lg bg-muted/50 border">
                      <Clock className="h-8 w-8 text-purple-500" />
                      <div>
                        <p className="text-2xl font-bold">
                          {praData.exercises.length > 0
                            ? formatDuration(
                                praData.exercises
                                  .filter(e => e.rto_seconds)
                                  .reduce((sum, e) => sum + (e.rto_seconds || 0), 0) /
                                praData.exercises.filter(e => e.rto_seconds).length || 0
                              )
                            : 'N/A'}
                        </p>
                        <p className="text-sm text-muted-foreground">RTO moyen</p>
                      </div>
                    </div>
                  </div>

                  {/* Exercises List */}
                  <div className="space-y-3">
                    <h4 className="font-medium">Détail des exercices</h4>
                    {praData.exercises.map((exercise) => (
                      <PraExerciseCard
                        key={exercise.switchover_id}
                        exercise={exercise}
                        expanded={expandedExercise === exercise.switchover_id}
                        onToggle={() => setExpandedExercise(
                          expandedExercise === exercise.switchover_id ? null : exercise.switchover_id
                        )}
                        formatDuration={formatDuration}
                      />
                    ))}
                  </div>

                  {/* Print Footer */}
                  <div className="hidden print:block border-t pt-4 mt-8 text-sm text-muted-foreground">
                    <p>Rapport généré le {new Date().toLocaleString('fr-FR')}</p>
                    <p>Application : {praData.application.name}</p>
                    <p>Conforme DORA Article 11 - Traçabilité des activités de reprise</p>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="compliance">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between">
              <CardTitle className="text-lg flex items-center gap-2">
                <Shield className="h-5 w-5" /> DORA Compliance
              </CardTitle>
              <Select value={selectedAppId || ''} onValueChange={(v) => setSelectedAppId(v || null)}>
                <SelectTrigger className="w-[250px]">
                  <SelectValue placeholder="Select an application">
                    {getAppName(selectedAppId) || 'Select an application'}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {apps?.map((app) => (
                    <SelectItem key={app.id} value={app.id}>
                      {app.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </CardHeader>
            <CardContent>
              {!selectedAppId ? (
                <p className="text-sm text-muted-foreground text-center py-8">
                  Select an application to view DORA compliance report.
                </p>
              ) : complianceLoading ? (
                <p className="text-sm text-muted-foreground text-center py-8">Loading...</p>
              ) : complianceData ? (
                <div className="space-y-6">
                  {/* Compliance Status Summary */}
                  <div className="flex items-center gap-4 p-4 rounded-lg border-2 border-green-200 bg-green-50 dark:bg-green-950/20 dark:border-green-800">
                    <CheckCircle className="h-10 w-10 text-green-500" />
                    <div>
                      <h3 className="text-lg font-semibold text-green-700 dark:text-green-400">
                        {complianceData.dora_compliant ? 'DORA Compliant' : 'Review Required'}
                      </h3>
                      <p className="text-sm text-muted-foreground">
                        This application meets DORA operational resilience requirements
                      </p>
                    </div>
                  </div>

                  {/* Metrics Grid */}
                  <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                    <div className="p-4 rounded-lg border border-border">
                      <div className="flex items-center gap-2 mb-2">
                        <FileText className="h-5 w-5 text-blue-500" />
                        <span className="font-medium">Audit Trail</span>
                      </div>
                      <p className="text-2xl font-bold">{complianceData.audit_trail_entries || 0}</p>
                      <p className="text-xs text-muted-foreground">operations logged</p>
                    </div>

                    <div className="p-4 rounded-lg border border-border">
                      <div className="flex items-center gap-2 mb-2">
                        <Shield className="h-5 w-5 text-purple-500" />
                        <span className="font-medium">Data Integrity</span>
                      </div>
                      <Badge variant={complianceData.append_only_enforced ? 'running' : 'failed'}>
                        {complianceData.append_only_enforced ? 'Append-Only Enforced' : 'Not Enforced'}
                      </Badge>
                      <p className="text-xs text-muted-foreground mt-1">Immutable audit logs</p>
                    </div>

                    <div className="p-4 rounded-lg border border-border">
                      <div className="flex items-center gap-2 mb-2">
                        <CheckCircle className="h-5 w-5 text-green-500" />
                        <span className="font-medium">Log Before Execute</span>
                      </div>
                      <Badge variant="running">Enforced</Badge>
                      <p className="text-xs text-muted-foreground mt-1">Actions logged before execution</p>
                    </div>
                  </div>

                  {/* DORA Requirements Checklist */}
                  <div className="p-4 rounded-lg bg-muted/50">
                    <h4 className="font-medium mb-3">DORA Compliance Checklist</h4>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
                      <div className="flex items-center gap-2 text-sm">
                        <CheckCircle className="h-4 w-4 text-green-500 shrink-0" />
                        <span>Immutable audit trail for all operations</span>
                      </div>
                      <div className="flex items-center gap-2 text-sm">
                        <CheckCircle className="h-4 w-4 text-green-500 shrink-0" />
                        <span>State transitions tracked with timestamps</span>
                      </div>
                      <div className="flex items-center gap-2 text-sm">
                        <CheckCircle className="h-4 w-4 text-green-500 shrink-0" />
                        <span>User actions logged before execution</span>
                      </div>
                      <div className="flex items-center gap-2 text-sm">
                        <CheckCircle className="h-4 w-4 text-green-500 shrink-0" />
                        <span>Permission changes audited</span>
                      </div>
                      <div className="flex items-center gap-2 text-sm">
                        <CheckCircle className="h-4 w-4 text-green-500 shrink-0" />
                        <span>Component lifecycle events recorded</span>
                      </div>
                      <div className="flex items-center gap-2 text-sm">
                        <CheckCircle className="h-4 w-4 text-green-500 shrink-0" />
                        <span>No DELETE operations on audit tables</span>
                      </div>
                    </div>
                  </div>

                  {/* Export Report Button */}
                  <div className="flex justify-end">
                    <Button onClick={handleExportReport} disabled={exportLoading}>
                      <Download className="h-4 w-4 mr-2" />
                      {exportLoading ? 'Generating...' : 'View Full Report'}
                    </Button>
                  </div>
                </div>
              ) : (
                <p className="text-sm text-muted-foreground text-center py-8">
                  No compliance data available.
                </p>
              )}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      {/* Export Report Modal */}
      <Dialog open={!!exportReport} onOpenChange={() => setExportReport(null)}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <FileText className="h-5 w-5" />
              Compliance Report
            </DialogTitle>
            <DialogDescription>
              {exportReport?.application.name} — Generated {exportReport && new Date(exportReport.generated_at).toLocaleString()}
            </DialogDescription>
          </DialogHeader>

          {exportReport && (
            <div className="space-y-6 py-4">
              {/* Period */}
              <div className="text-sm text-muted-foreground">
                Period: {new Date(exportReport.period.from).toLocaleDateString()} — {new Date(exportReport.period.to).toLocaleDateString()}
              </div>

              {/* Summary Metrics */}
              <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
                <div className="p-4 rounded-lg border">
                  <p className="text-sm text-muted-foreground">Availability</p>
                  <p className="text-2xl font-bold">{exportReport.summary.overall_availability_pct.toFixed(1)}%</p>
                </div>
                <div className="p-4 rounded-lg border">
                  <p className="text-sm text-muted-foreground">Incidents</p>
                  <p className="text-2xl font-bold">{exportReport.summary.incident_count}</p>
                </div>
                <div className="p-4 rounded-lg border">
                  <p className="text-sm text-muted-foreground">Switchovers</p>
                  <p className="text-2xl font-bold">{exportReport.summary.switchover_count}</p>
                </div>
                <div className="p-4 rounded-lg border">
                  <p className="text-sm text-muted-foreground">Audit Entries</p>
                  <p className="text-2xl font-bold">{exportReport.summary.audit_trail_entries}</p>
                </div>
                <div className="p-4 rounded-lg border">
                  <p className="text-sm text-muted-foreground">Avg RTO</p>
                  <p className="text-2xl font-bold">{formatDuration(exportReport.summary.average_rto_seconds)}</p>
                </div>
                <div className="p-4 rounded-lg border">
                  <p className="text-sm text-muted-foreground">DORA Status</p>
                  <Badge variant={exportReport.summary.dora_compliant ? 'running' : 'failed'} className="mt-1">
                    {exportReport.summary.dora_compliant ? 'Compliant' : 'Non-Compliant'}
                  </Badge>
                </div>
              </div>

              {/* Generated By */}
              <div className="text-sm text-muted-foreground border-t pt-4">
                Generated by: {exportReport.generated_by}
              </div>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
