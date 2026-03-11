import { useState, useMemo } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  History,
  Calendar,
  Server,
  Plus,
  Minus,
  RefreshCw,
  Clock,
  GitCompare,
  Loader2,
  CalendarClock,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import {
  useDiscoveryReports,
  useSnapshotSchedules,
  useScheduledSnapshots,
  type DiscoveryReport,
} from '@/api/discovery';
import { ScheduleModal } from './ScheduleModal';

interface HistoryModalProps {
  open: boolean;
  onClose: () => void;
}

interface DiffResult {
  added: string[];
  removed: string[];
  modified: string[];
}

export function HistoryModal({ open, onClose }: HistoryModalProps) {
  const [selectedReports, setSelectedReports] = useState<string[]>([]);
  const [comparing, setComparing] = useState(false);
  const [diffResult, setDiffResult] = useState<DiffResult | null>(null);
  const [scheduleModalOpen, setScheduleModalOpen] = useState(false);

  const { data: reports, isLoading } = useDiscoveryReports();
  const { data: schedules } = useSnapshotSchedules();
  const { data: scheduledSnapshots } = useScheduledSnapshots();

  const activeScheduleCount = schedules?.filter((s) => s.enabled).length || 0;

  // Group reports by agent/hostname
  const reportsByAgent = useMemo(() => {
    if (!reports) return new Map<string, DiscoveryReport[]>();

    const grouped = new Map<string, DiscoveryReport[]>();
    reports.forEach((report) => {
      const key = report.hostname || report.agent_id;
      if (!grouped.has(key)) {
        grouped.set(key, []);
      }
      grouped.get(key)!.push(report);
    });

    // Sort each group by date (newest first)
    grouped.forEach((list) => {
      list.sort((a, b) => new Date(b.scanned_at).getTime() - new Date(a.scanned_at).getTime());
    });

    return grouped;
  }, [reports]);

  const toggleReportSelection = (reportId: string) => {
    setSelectedReports((prev) => {
      if (prev.includes(reportId)) {
        return prev.filter((id) => id !== reportId);
      }
      if (prev.length >= 2) {
        // Replace the oldest selection
        return [prev[1], reportId];
      }
      return [...prev, reportId];
    });
    setDiffResult(null);
  };

  const handleCompare = () => {
    if (selectedReports.length !== 2) return;

    setComparing(true);

    // Simulate comparison (in real implementation, this would fetch full reports)
    setTimeout(() => {
      // Mock diff result for demonstration
      setDiffResult({
        added: ['java.exe (port 8080)', 'python3 (port 5000)'],
        removed: ['old-service.exe'],
        modified: ['nginx: port 80 → 443'],
      });
      setComparing(false);
    }, 500);
  };

  const formatDate = (dateStr: string) => {
    const date = new Date(dateStr);
    return {
      date: date.toLocaleDateString('fr-FR', { day: '2-digit', month: 'short', year: 'numeric' }),
      time: date.toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit' }),
    };
  };

  const getRelativeTime = (dateStr: string) => {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;
    return formatDate(dateStr).date;
  };

  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="max-w-3xl max-h-[85vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <History className="h-5 w-5" />
            Discovery History
          </DialogTitle>
          <DialogDescription>
            View past scans and compare changes between snapshots.
          </DialogDescription>
        </DialogHeader>

        <Tabs defaultValue="history" className="flex-1 flex flex-col overflow-hidden">
          <div className="flex items-center justify-between mb-2">
            <TabsList className="grid w-full grid-cols-3">
              <TabsTrigger value="history" className="gap-2">
                <Clock className="h-4 w-4" />
                History
              </TabsTrigger>
              <TabsTrigger value="scheduled" className="gap-2">
                <CalendarClock className="h-4 w-4" />
                Scheduled
                {activeScheduleCount > 0 && (
                  <Badge variant="secondary" className="text-[10px] h-4 px-1 ml-1">
                    {activeScheduleCount}
                  </Badge>
                )}
              </TabsTrigger>
              <TabsTrigger value="compare" className="gap-2">
                <GitCompare className="h-4 w-4" />
                Compare
              </TabsTrigger>
            </TabsList>
          </div>

          <TabsContent value="history" className="flex-1 overflow-auto mt-4">
            {isLoading ? (
              <div className="flex items-center justify-center py-8">
                <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
              </div>
            ) : reportsByAgent.size === 0 ? (
              <div className="text-center py-8 text-muted-foreground">
                <History className="h-12 w-12 mx-auto mb-3 opacity-50" />
                <p>No discovery history yet.</p>
                <p className="text-sm">Scan agents to start building history.</p>
              </div>
            ) : (
              <ScrollArea className="h-[400px] pr-4">
                <div className="space-y-6">
                  {Array.from(reportsByAgent.entries()).map(([hostname, agentReports]) => (
                    <div key={hostname}>
                      <div className="flex items-center gap-2 mb-3">
                        <Server className="h-4 w-4 text-muted-foreground" />
                        <span className="font-medium">{hostname}</span>
                        <Badge variant="secondary" className="text-[10px]">
                          {agentReports.length} scan{agentReports.length > 1 ? 's' : ''}
                        </Badge>
                      </div>
                      <div className="space-y-2 ml-6">
                        {agentReports.slice(0, 5).map((report, idx) => {
                          const { date, time } = formatDate(report.scanned_at);
                          return (
                            <div
                              key={report.id}
                              className={cn(
                                'flex items-center gap-3 p-2 rounded-md border',
                                idx === 0 ? 'bg-primary/5 border-primary/30' : 'bg-card'
                              )}
                            >
                              <Calendar className="h-4 w-4 text-muted-foreground" />
                              <div className="flex-1">
                                <span className="text-sm">{date}</span>
                                <span className="text-xs text-muted-foreground ml-2">{time}</span>
                              </div>
                              <span className="text-xs text-muted-foreground">
                                {getRelativeTime(report.scanned_at)}
                              </span>
                              {idx === 0 && (
                                <Badge className="text-[10px]">Latest</Badge>
                              )}
                            </div>
                          );
                        })}
                        {agentReports.length > 5 && (
                          <p className="text-xs text-muted-foreground text-center py-1">
                            +{agentReports.length - 5} more scans
                          </p>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </ScrollArea>
            )}
          </TabsContent>

          <TabsContent value="scheduled" className="flex-1 overflow-auto mt-4">
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <p className="text-sm text-muted-foreground">
                  Automatic snapshots captured by scheduled scans.
                </p>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setScheduleModalOpen(true)}
                  className="gap-1"
                >
                  <CalendarClock className="h-4 w-4" />
                  Manage Schedules
                </Button>
              </div>

              {!scheduledSnapshots || scheduledSnapshots.length === 0 ? (
                <div className="text-center py-8 text-muted-foreground">
                  <CalendarClock className="h-12 w-12 mx-auto mb-3 opacity-50" />
                  <p>No scheduled snapshots yet.</p>
                  <p className="text-sm">Create a schedule to automatically capture snapshots.</p>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setScheduleModalOpen(true)}
                    className="mt-4 gap-1"
                  >
                    <Plus className="h-4 w-4" />
                    Create Schedule
                  </Button>
                </div>
              ) : (
                <ScrollArea className="h-[350px] pr-4">
                  <div className="space-y-3">
                    {scheduledSnapshots.map((snapshot) => {
                      const capturedDate = new Date(snapshot.captured_at);
                      return (
                        <div
                          key={snapshot.id}
                          className="p-3 rounded-lg border bg-card hover:shadow-sm transition-all"
                        >
                          <div className="flex items-center justify-between">
                            <div>
                              <div className="flex items-center gap-2">
                                <CalendarClock className="h-4 w-4 text-muted-foreground" />
                                <span className="font-medium text-sm">{snapshot.schedule_name}</span>
                              </div>
                              <div className="flex items-center gap-3 mt-1 text-xs text-muted-foreground">
                                <span>
                                  {capturedDate.toLocaleDateString('fr-FR', {
                                    day: '2-digit',
                                    month: 'short',
                                    year: 'numeric',
                                  })}{' '}
                                  {capturedDate.toLocaleTimeString('fr-FR', {
                                    hour: '2-digit',
                                    minute: '2-digit',
                                  })}
                                </span>
                                <span className="flex items-center gap-1">
                                  <Server className="h-3 w-3" />
                                  {snapshot.agent_ids.length} agent
                                  {snapshot.agent_ids.length !== 1 ? 's' : ''}
                                </span>
                              </div>
                            </div>
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={() => {
                                if (selectedReports.length === 0) {
                                  setSelectedReports([snapshot.report_ids[0]]);
                                } else {
                                  setSelectedReports([selectedReports[0], snapshot.report_ids[0]]);
                                }
                              }}
                              className="text-xs"
                            >
                              Select for Compare
                            </Button>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </ScrollArea>
              )}
            </div>
          </TabsContent>

          <TabsContent value="compare" className="flex-1 overflow-auto mt-4">
            <div className="space-y-4">
              <p className="text-sm text-muted-foreground">
                Select 2 snapshots to compare. Click on reports to select them.
              </p>

              {/* Selection */}
              <div className="grid grid-cols-2 gap-4">
                <div className="p-3 rounded-lg border-2 border-dashed text-center">
                  {selectedReports[0] ? (
                    <div className="text-sm font-medium">
                      {reports?.find((r) => r.id === selectedReports[0])?.hostname}
                      <br />
                      <span className="text-xs text-muted-foreground">
                        {formatDate(reports?.find((r) => r.id === selectedReports[0])?.scanned_at || '').date}
                      </span>
                    </div>
                  ) : (
                    <span className="text-xs text-muted-foreground">Select first snapshot</span>
                  )}
                </div>
                <div className="p-3 rounded-lg border-2 border-dashed text-center">
                  {selectedReports[1] ? (
                    <div className="text-sm font-medium">
                      {reports?.find((r) => r.id === selectedReports[1])?.hostname}
                      <br />
                      <span className="text-xs text-muted-foreground">
                        {formatDate(reports?.find((r) => r.id === selectedReports[1])?.scanned_at || '').date}
                      </span>
                    </div>
                  ) : (
                    <span className="text-xs text-muted-foreground">Select second snapshot</span>
                  )}
                </div>
              </div>

              {/* Report list for selection */}
              <ScrollArea className="h-[200px] border rounded-md p-2">
                <div className="space-y-1">
                  {reports?.map((report) => {
                    const { date, time } = formatDate(report.scanned_at);
                    const isSelected = selectedReports.includes(report.id);
                    return (
                      <button
                        key={report.id}
                        onClick={() => toggleReportSelection(report.id)}
                        className={cn(
                          'w-full flex items-center gap-3 p-2 rounded-md text-left transition-all',
                          isSelected
                            ? 'bg-primary/10 border border-primary'
                            : 'hover:bg-accent border border-transparent'
                        )}
                      >
                        <Server className="h-4 w-4 text-muted-foreground" />
                        <span className="flex-1 text-sm">{report.hostname}</span>
                        <span className="text-xs text-muted-foreground">{date} {time}</span>
                      </button>
                    );
                  })}
                </div>
              </ScrollArea>

              {/* Compare button */}
              <Button
                onClick={handleCompare}
                disabled={selectedReports.length !== 2 || comparing}
                className="w-full gap-2"
              >
                {comparing ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <GitCompare className="h-4 w-4" />
                )}
                Compare Snapshots
              </Button>

              {/* Diff results */}
              {diffResult && (
                <div className="space-y-3 p-4 rounded-lg border bg-card">
                  <h4 className="font-medium text-sm">Differences</h4>

                  {diffResult.added.length > 0 && (
                    <div className="space-y-1">
                      <span className="text-xs font-medium text-emerald-600 flex items-center gap-1">
                        <Plus className="h-3 w-3" /> Added ({diffResult.added.length})
                      </span>
                      {diffResult.added.map((item, i) => (
                        <div key={i} className="text-xs text-muted-foreground ml-4">
                          + {item}
                        </div>
                      ))}
                    </div>
                  )}

                  {diffResult.removed.length > 0 && (
                    <div className="space-y-1">
                      <span className="text-xs font-medium text-red-600 flex items-center gap-1">
                        <Minus className="h-3 w-3" /> Removed ({diffResult.removed.length})
                      </span>
                      {diffResult.removed.map((item, i) => (
                        <div key={i} className="text-xs text-muted-foreground ml-4">
                          - {item}
                        </div>
                      ))}
                    </div>
                  )}

                  {diffResult.modified.length > 0 && (
                    <div className="space-y-1">
                      <span className="text-xs font-medium text-amber-600 flex items-center gap-1">
                        <RefreshCw className="h-3 w-3" /> Modified ({diffResult.modified.length})
                      </span>
                      {diffResult.modified.map((item, i) => (
                        <div key={i} className="text-xs text-muted-foreground ml-4">
                          ~ {item}
                        </div>
                      ))}
                    </div>
                  )}

                  {diffResult.added.length === 0 && diffResult.removed.length === 0 && diffResult.modified.length === 0 && (
                    <p className="text-sm text-muted-foreground text-center py-2">
                      No differences found.
                    </p>
                  )}
                </div>
              )}
            </div>
          </TabsContent>
        </Tabs>
      </DialogContent>

      {/* Schedule Management Modal */}
      <ScheduleModal
        open={scheduleModalOpen}
        onClose={() => setScheduleModalOpen(false)}
      />
    </Dialog>
  );
}
