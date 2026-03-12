import { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import {
  Download,
  FileJson,
  FileText,
  Table,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { useDiscoveryStore } from '@/stores/discovery';

interface ExportModalProps {
  open: boolean;
  onClose: () => void;
}

type ExportFormat = 'json' | 'yaml' | 'csv' | 'markdown';

interface ExportOptions {
  processes: boolean;
  listeners: boolean;
  connections: boolean;
  services: boolean;
  scheduledJobs: boolean;
  firewallRules: boolean;
}

const FORMAT_OPTIONS: { id: ExportFormat; label: string; icon: React.ComponentType<{ className?: string }>; description: string }[] = [
  { id: 'json', label: 'JSON', icon: FileJson, description: 'Complete data, machine-readable' },
  { id: 'yaml', label: 'YAML', icon: FileText, description: 'Human-readable, AppControl legacy format' },
  { id: 'csv', label: 'CSV', icon: Table, description: 'For Excel/spreadsheet analysis' },
  { id: 'markdown', label: 'Markdown', icon: FileText, description: 'Documentation format' },
];

export function ExportModal({ open, onClose }: ExportModalProps) {
  const [format, setFormat] = useState<ExportFormat>('json');
  const [options, setOptions] = useState<ExportOptions>({
    processes: true,
    listeners: true,
    connections: true,
    services: true,
    scheduledJobs: true,
    firewallRules: false,
  });

  const correlationResult = useDiscoveryStore((s) => s.correlationResult);
  const selectedAgentIds = useDiscoveryStore((s) => s.selectedAgentIds);

  const toggleOption = (key: keyof ExportOptions) => {
    setOptions((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  const generateExport = () => {
    if (!correlationResult) return;

    let content = '';
    let filename = '';
    let mimeType = '';

    const data = {
      exported_at: new Date().toISOString(),
      agents: selectedAgentIds,
      ...(options.processes && { services: correlationResult.services }),
      ...(options.connections && { dependencies: correlationResult.dependencies }),
      ...(options.connections && { unresolved_connections: correlationResult.unresolved_connections }),
      ...(options.scheduledJobs && { scheduled_jobs: correlationResult.scheduled_jobs }),
    };

    switch (format) {
      case 'json':
        content = JSON.stringify(data, null, 2);
        filename = `discovery-export-${Date.now()}.json`;
        mimeType = 'application/json';
        break;

      case 'yaml':
        content = generateYAML(data);
        filename = `discovery-export-${Date.now()}.yaml`;
        mimeType = 'text/yaml';
        break;

      case 'csv':
        content = generateCSV(correlationResult.services as unknown as Array<Record<string, unknown>>);
        filename = `discovery-export-${Date.now()}.csv`;
        mimeType = 'text/csv';
        break;

      case 'markdown':
        content = generateMarkdown(data);
        filename = `discovery-export-${Date.now()}.md`;
        mimeType = 'text/markdown';
        break;
    }

    // Download file
    const blob = new Blob([content], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);

    onClose();
  };

  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Download className="h-5 w-5" />
            Export Discovery Data
          </DialogTitle>
          <DialogDescription>
            Export raw discovery data for offline analysis or documentation.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6 py-4">
          {/* Format selection */}
          <div className="space-y-3">
            <Label>Format</Label>
            <div className="grid grid-cols-2 gap-2">
              {FORMAT_OPTIONS.map((opt) => {
                const Icon = opt.icon;
                const selected = format === opt.id;
                return (
                  <button
                    key={opt.id}
                    onClick={() => setFormat(opt.id)}
                    className={cn(
                      'flex items-start gap-3 p-3 rounded-lg border text-left transition-all',
                      selected
                        ? 'border-primary bg-primary/5 ring-1 ring-primary'
                        : 'border-border hover:bg-accent'
                    )}
                  >
                    <Icon className={cn('h-5 w-5 mt-0.5', selected ? 'text-primary' : 'text-muted-foreground')} />
                    <div>
                      <div className={cn('font-medium text-sm', selected && 'text-primary')}>
                        {opt.label}
                      </div>
                      <div className="text-xs text-muted-foreground">
                        {opt.description}
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Content selection */}
          <div className="space-y-3">
            <Label>Content to export</Label>
            <div className="space-y-2">
              {[
                { key: 'processes' as const, label: 'Services/Processes', count: correlationResult?.services.length || 0 },
                { key: 'connections' as const, label: 'Dependencies & Connections', count: (correlationResult?.dependencies.length || 0) + (correlationResult?.unresolved_connections.length || 0) },
                { key: 'scheduledJobs' as const, label: 'Scheduled Jobs', count: correlationResult?.scheduled_jobs.length || 0 },
              ].map((item) => (
                <label
                  key={item.key}
                  className="flex items-center gap-3 p-2 rounded-md hover:bg-accent cursor-pointer"
                >
                  <input
                    type="checkbox"
                    checked={options[item.key]}
                    onChange={() => toggleOption(item.key)}
                    className="h-4 w-4 rounded border-gray-300 text-primary focus:ring-primary"
                  />
                  <span className="flex-1 text-sm">{item.label}</span>
                  <span className="text-xs text-muted-foreground">{item.count}</span>
                </label>
              ))}
            </div>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={generateExport} className="gap-2">
            <Download className="h-4 w-4" />
            Export
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// Helper functions for different formats

function generateYAML(data: Record<string, unknown>): string {
  const lines: string[] = [];
  lines.push(`# Discovery Export - ${data.exported_at}`);
  lines.push(`# Agents: ${(data.agents as string[])?.join(', ')}`);
  lines.push('');

  if (data.services && Array.isArray(data.services)) {
    lines.push('Components:');
    (data.services as Array<Record<string, unknown>>).forEach((svc) => {
      lines.push(`- Name: ${svc.suggested_name || svc.process_name}`);
      lines.push(`  Agent: ${svc.hostname}`);
      lines.push(`  Group: ${(svc.technology_hint as Record<string, string>)?.layer || svc.component_type}`);
      if (svc.technology_hint) {
        const tech = svc.technology_hint as Record<string, string>;
        lines.push(`  Icon:`);
        lines.push(`    SystemName: ${tech.icon}`);
      }
      if ((svc.ports as number[])?.length > 0) {
        lines.push(`  Ports: [${(svc.ports as number[]).join(', ')}]`);
      }
      if (svc.command_suggestion) {
        const cmd = svc.command_suggestion as Record<string, string>;
        lines.push(`  Actions:`);
        if (cmd.check_cmd) {
          lines.push(`  - Name: check`);
          lines.push(`    Type: check`);
          lines.push(`    Value: ${cmd.check_cmd}`);
        }
        if (cmd.start_cmd) {
          lines.push(`  - Name: start`);
          lines.push(`    Type: start`);
          lines.push(`    Value: ${cmd.start_cmd}`);
        }
        if (cmd.stop_cmd) {
          lines.push(`  - Name: stop`);
          lines.push(`    Type: stop`);
          lines.push(`    Value: ${cmd.stop_cmd}`);
        }
      }
      lines.push('');
    });
  }

  return lines.join('\n');
}

function generateCSV(services: Array<Record<string, unknown>>): string {
  const headers = ['Name', 'Process', 'Hostname', 'Type', 'Layer', 'Ports', 'Identified'];
  const rows = services.map((svc) => {
    const tech = svc.technology_hint as Record<string, string> | undefined;
    return [
      svc.suggested_name || svc.process_name,
      svc.process_name,
      svc.hostname,
      svc.component_type,
      tech?.layer || '',
      (svc.ports as number[])?.join(';') || '',
      tech ? 'Yes' : 'No',
    ].map((v) => `"${String(v).replace(/"/g, '""')}"`).join(',');
  });
  return [headers.join(','), ...rows].join('\n');
}

function generateMarkdown(data: Record<string, unknown>): string {
  const lines: string[] = [];
  lines.push(`# Discovery Export`);
  lines.push('');
  lines.push(`**Exported:** ${data.exported_at}`);
  lines.push(`**Agents:** ${(data.agents as string[])?.join(', ')}`);
  lines.push('');

  if (data.services && Array.isArray(data.services)) {
    lines.push('## Services');
    lines.push('');
    lines.push('| Name | Host | Type | Ports | Identified |');
    lines.push('|------|------|------|-------|------------|');
    (data.services as Array<Record<string, unknown>>).forEach((svc) => {
      const tech = svc.technology_hint as Record<string, string> | undefined;
      lines.push(`| ${svc.suggested_name || svc.process_name} | ${svc.hostname} | ${tech?.layer || svc.component_type} | ${(svc.ports as number[])?.join(', ') || '-'} | ${tech ? 'Yes' : 'No'} |`);
    });
    lines.push('');
  }

  if (data.dependencies && Array.isArray(data.dependencies)) {
    lines.push('## Dependencies');
    lines.push('');
    (data.dependencies as Array<Record<string, unknown>>).forEach((dep) => {
      lines.push(`- ${dep.from_process} → ${dep.to_process} (${dep.inferred_via})`);
    });
    lines.push('');
  }

  if (data.scheduled_jobs && Array.isArray(data.scheduled_jobs)) {
    lines.push('## Scheduled Jobs');
    lines.push('');
    lines.push('| Name | Schedule | Command |');
    lines.push('|------|----------|---------|');
    (data.scheduled_jobs as Array<Record<string, unknown>>).forEach((job) => {
      lines.push(`| ${job.name} | ${job.schedule} | \`${job.command}\` |`);
    });
    lines.push('');
  }

  return lines.join('\n');
}
