import { useState, useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Progress } from '@/components/ui/progress';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  CheckCircle,
  XCircle,
  HelpCircle,
  ArrowRight,
  ArrowLeft,
  Sparkles,
  ChevronRight,
  Server,
  Database,
  Layers,
  Globe,
  Cog,
  Search,
  Calendar,
  Box,
  Download,
  History,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { useDiscoveryStore, type TriageStatus } from '@/stores/discovery';
import { TECHNOLOGY_ICONS } from '@/lib/colors';
import { AIAssistantModal } from './AIAssistantModal';
import { ExportModal } from './ExportModal';
import { HistoryModal } from './HistoryModal';

const iconMap: Record<string, React.ComponentType<{ className?: string }>> = {
  Database, Layers, Server, Globe, Cog, Search, Calendar, Box,
};

interface ServiceCardProps {
  index: number;
  onMove: (status: TriageStatus) => void;
  compact?: boolean;
}

function ServiceCard({ index, onMove, compact }: ServiceCardProps) {
  const correlationResult = useDiscoveryStore((s) => s.correlationResult);
  const serviceTriageStatus = useDiscoveryStore((s) => s.serviceTriageStatus);
  const isServiceIdentified = useDiscoveryStore((s) => s.isServiceIdentified);

  const service = correlationResult?.services[index];
  if (!service) return null;

  const status = serviceTriageStatus.get(index) || 'pending';
  const identified = isServiceIdentified(index);
  const techHint = service.technology_hint;
  const techInfo = techHint?.icon ? TECHNOLOGY_ICONS[techHint.icon] : null;

  const IconComponent = techInfo?.icon ? iconMap[techInfo.icon] || Box : Box;
  const color = techInfo?.color || '#64748b';
  const displayName = techHint?.display_name || service.process_name;
  const layer = techHint?.layer || service.component_type;

  return (
    <div
      className={cn(
        'p-3 rounded-lg border bg-card hover:shadow-md transition-all cursor-pointer group',
        status === 'include' && 'border-emerald-300 bg-emerald-50/50',
        status === 'ignore' && 'border-slate-300 bg-slate-50/50 opacity-60',
        status === 'pending' && !identified && 'border-amber-300 bg-amber-50/50',
        status === 'pending' && identified && 'border-blue-300 bg-blue-50/50',
      )}
    >
      <div className="flex items-start gap-3">
        {/* Icon */}
        <div
          className="w-8 h-8 rounded-md flex items-center justify-center flex-shrink-0"
          style={{ backgroundColor: `${color}20` }}
        >
          <IconComponent className="h-4 w-4" style={{ color }} />
        </div>

        {/* Content */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="font-medium text-sm truncate">{displayName}</span>
            {!identified && (
              <Badge variant="outline" className="text-[10px] h-4 px-1 text-amber-600 border-amber-300">
                <HelpCircle className="h-2.5 w-2.5 mr-0.5" />
                ?
              </Badge>
            )}
          </div>
          <div className="text-xs text-muted-foreground truncate">
            {service.hostname}
            {service.ports.length > 0 && (
              <span className="ml-1">
                ({service.ports.slice(0, 3).map(p => `:${p}`).join(', ')}
                {service.ports.length > 3 && '...'})
              </span>
            )}
          </div>
          {!compact && (
            <Badge variant="secondary" className="text-[10px] mt-1 h-4">
              {layer}
            </Badge>
          )}
        </div>

        {/* Actions */}
        <div className="flex flex-col gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
          {status !== 'include' && (
            <Button
              size="icon"
              variant="ghost"
              className="h-6 w-6 text-emerald-600 hover:bg-emerald-100"
              onClick={(e) => { e.stopPropagation(); onMove('include'); }}
              title="Include"
            >
              <CheckCircle className="h-4 w-4" />
            </Button>
          )}
          {status !== 'ignore' && (
            <Button
              size="icon"
              variant="ghost"
              className="h-6 w-6 text-slate-500 hover:bg-slate-100"
              onClick={(e) => { e.stopPropagation(); onMove('ignore'); }}
              title="Ignore"
            >
              <XCircle className="h-4 w-4" />
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}

export function TriagePhase() {
  const [aiModalOpen, setAiModalOpen] = useState(false);
  const [selectedForAI, setSelectedForAI] = useState<number[]>([]);
  const [exportModalOpen, setExportModalOpen] = useState(false);
  const [historyModalOpen, setHistoryModalOpen] = useState(false);

  const {
    correlationResult,
    serviceTriageStatus,
    setServiceTriageStatus,
    bulkSetTriageStatus,
    getTriageCounts,
    getTriageProgress,
    getUnidentifiedServices,
    getIdentifiedServices,
    setPhase,
  } = useDiscoveryStore();

  const services = correlationResult?.services || [];
  const counts = getTriageCounts();
  const progress = getTriageProgress();
  const unidentifiedIndices = getUnidentifiedServices();
  const identifiedIndices = getIdentifiedServices();

  // Group services by status
  const { included, ignored, pendingIdentified, pendingUnidentified } = useMemo(() => {
    const included: number[] = [];
    const ignored: number[] = [];
    const pendingIdentified: number[] = [];
    const pendingUnidentified: number[] = [];

    services.forEach((_, i) => {
      const status = serviceTriageStatus.get(i) || 'pending';
      const identified = !!services[i]?.technology_hint;

      if (status === 'include') included.push(i);
      else if (status === 'ignore') ignored.push(i);
      else if (identified) pendingIdentified.push(i);
      else pendingUnidentified.push(i);
    });

    return { included, ignored, pendingIdentified, pendingUnidentified };
  }, [services, serviceTriageStatus]);

  const handleIncludeAllIdentified = () => {
    bulkSetTriageStatus(pendingIdentified, 'include');
  };

  const handleOpenAIAssist = () => {
    setSelectedForAI(pendingUnidentified);
    setAiModalOpen(true);
  };

  const canProceed = counts.included > 0;

  return (
    <div className="space-y-6">
      {/* Header with progress */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold flex items-center gap-2">
            Triage Components
          </h1>
          <p className="text-muted-foreground mt-1">
            Sort discovered components: include in app, ignore, or identify unknowns.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setHistoryModalOpen(true)}
            className="gap-1"
          >
            <History className="h-4 w-4" />
            History
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setExportModalOpen(true)}
            className="gap-1"
          >
            <Download className="h-4 w-4" />
            Export
          </Button>
          <div className="w-px h-6 bg-border mx-1" />
          <Button
            variant="outline"
            onClick={() => setPhase('scan')}
            className="gap-2"
          >
            <ArrowLeft className="h-4 w-4" />
            Back
          </Button>
          <Button
            onClick={() => setPhase('topology')}
            disabled={!canProceed}
            className="gap-2"
          >
            Build Map
            <ArrowRight className="h-4 w-4" />
          </Button>
        </div>
      </div>

      {/* Progress bar */}
      <Card>
        <CardContent className="p-4">
          <div className="flex items-center justify-between mb-2">
            <span className="text-sm font-medium">Triage Progress</span>
            <span className="text-sm text-muted-foreground">
              {counts.included + counts.ignored} / {counts.total} sorted
            </span>
          </div>
          <Progress value={progress} className="h-2" />
          <div className="flex items-center gap-4 mt-2 text-xs">
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-emerald-500" />
              <span className="text-emerald-700">{counts.included} included</span>
            </span>
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-slate-400" />
              <span className="text-slate-600">{counts.ignored} ignored</span>
            </span>
            <span className="flex items-center gap-1">
              <span className="w-2 h-2 rounded-full bg-amber-400" />
              <span className="text-amber-700">{counts.pending} pending</span>
            </span>
          </div>
        </CardContent>
      </Card>

      {/* Three-column layout */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Column 1: To Include */}
        <Card className="border-emerald-200">
          <CardHeader className="pb-3">
            <CardTitle className="text-base flex items-center justify-between">
              <span className="flex items-center gap-2 text-emerald-700">
                <CheckCircle className="h-4 w-4" />
                Include ({included.length})
              </span>
            </CardTitle>
          </CardHeader>
          <CardContent>
            <ScrollArea className="h-[400px] pr-3">
              <div className="space-y-2">
                {included.length === 0 ? (
                  <p className="text-sm text-muted-foreground text-center py-8">
                    No components selected yet.
                    <br />
                    Click <CheckCircle className="h-3 w-3 inline mx-1" /> on components to include them.
                  </p>
                ) : (
                  included.map((i) => (
                    <ServiceCard
                      key={i}
                      index={i}
                      onMove={(status) => setServiceTriageStatus(i, status)}
                      compact
                    />
                  ))
                )}
              </div>
            </ScrollArea>
          </CardContent>
        </Card>

        {/* Column 2: Pending (identified + unidentified) */}
        <Card className="border-blue-200">
          <CardHeader className="pb-3">
            <CardTitle className="text-base flex items-center justify-between">
              <span className="flex items-center gap-2 text-blue-700">
                <Server className="h-4 w-4" />
                Pending ({pendingIdentified.length + pendingUnidentified.length})
              </span>
              {pendingIdentified.length > 0 && (
                <Button
                  size="sm"
                  variant="outline"
                  className="h-7 text-xs gap-1"
                  onClick={handleIncludeAllIdentified}
                >
                  <CheckCircle className="h-3 w-3" />
                  Include {pendingIdentified.length} identified
                </Button>
              )}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <ScrollArea className="h-[400px] pr-3">
              <div className="space-y-2">
                {/* Unidentified section */}
                {pendingUnidentified.length > 0 && (
                  <div className="mb-4">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-xs font-medium text-amber-700 flex items-center gap-1">
                        <HelpCircle className="h-3 w-3" />
                        Unidentified ({pendingUnidentified.length})
                      </span>
                      <Button
                        size="sm"
                        variant="outline"
                        className="h-6 text-[10px] gap-1 text-violet-600 border-violet-300 hover:bg-violet-50"
                        onClick={handleOpenAIAssist}
                      >
                        <Sparkles className="h-3 w-3" />
                        AI Assist
                      </Button>
                    </div>
                    {pendingUnidentified.map((i) => (
                      <ServiceCard
                        key={i}
                        index={i}
                        onMove={(status) => setServiceTriageStatus(i, status)}
                      />
                    ))}
                  </div>
                )}

                {/* Identified section */}
                {pendingIdentified.length > 0 && (
                  <div>
                    <span className="text-xs font-medium text-blue-700 flex items-center gap-1 mb-2">
                      <CheckCircle className="h-3 w-3" />
                      Identified ({pendingIdentified.length})
                    </span>
                    {pendingIdentified.map((i) => (
                      <ServiceCard
                        key={i}
                        index={i}
                        onMove={(status) => setServiceTriageStatus(i, status)}
                      />
                    ))}
                  </div>
                )}

                {pendingIdentified.length === 0 && pendingUnidentified.length === 0 && (
                  <p className="text-sm text-muted-foreground text-center py-8">
                    All components have been sorted.
                  </p>
                )}
              </div>
            </ScrollArea>
          </CardContent>
        </Card>

        {/* Column 3: Ignored */}
        <Card className="border-slate-200">
          <CardHeader className="pb-3">
            <CardTitle className="text-base flex items-center justify-between">
              <span className="flex items-center gap-2 text-slate-600">
                <XCircle className="h-4 w-4" />
                Ignore ({ignored.length})
              </span>
            </CardTitle>
          </CardHeader>
          <CardContent>
            <ScrollArea className="h-[400px] pr-3">
              <div className="space-y-2">
                {ignored.length === 0 ? (
                  <p className="text-sm text-muted-foreground text-center py-8">
                    Components you want to exclude from the app.
                    <br />
                    (System processes, monitoring agents, etc.)
                  </p>
                ) : (
                  ignored.map((i) => (
                    <ServiceCard
                      key={i}
                      index={i}
                      onMove={(status) => setServiceTriageStatus(i, status)}
                      compact
                    />
                  ))
                )}
              </div>
            </ScrollArea>
          </CardContent>
        </Card>
      </div>

      {/* AI Assistant Modal */}
      <AIAssistantModal
        open={aiModalOpen}
        onClose={() => setAiModalOpen(false)}
        serviceIndices={selectedForAI}
      />

      {/* Export Modal */}
      <ExportModal
        open={exportModalOpen}
        onClose={() => setExportModalOpen(false)}
      />

      {/* History Modal */}
      <HistoryModal
        open={historyModalOpen}
        onClose={() => setHistoryModalOpen(false)}
      />
    </div>
  );
}
