import { useState } from 'react';
import { Upload, FileJson, FileSpreadsheet, Loader2, CheckCircle, AlertTriangle } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { useApps } from '@/api/apps';
import client from '@/api/client';
import { useMutation } from '@tanstack/react-query';
import { cn } from '@/lib/utils';

type Source = 'cmdb' | 'xl' | 'flows' | 'incidents';
type Format = 'json' | 'csv';

interface IngestionReport {
  source: string;
  created: number;
  updated: number;
  skipped: number;
  errors: { item?: string; message: string }[];
}

interface Props {
  className?: string;
}

const sourceLabels: Record<Source, string> = {
  cmdb: 'CMDB',
  xl: 'XL Release / XL Deploy',
  flows: 'Référentiel de flux',
  incidents: 'ITSM / incidents',
};

const sourceTemplates: Record<Source, { json: string; csv: string }> = {
  cmdb: {
    json: JSON.stringify(
      {
        application_id: 'PASTE-APPLICATION-UUID',
        source: 'servicenow',
        components: [
          {
            name: 'billing-api',
            component_type: 'service',
            host: 'srv-12.prod',
            tags: ['java', 'tier-1'],
          },
        ],
      },
      null,
      2,
    ),
    csv: 'name,component_type,host,description,display_name,tags\nbilling-api,service,srv-12.prod,Billing public API,Billing API,java;tier-1\n',
  },
  xl: {
    json: JSON.stringify(
      {
        application_id: 'PASTE-APPLICATION-UUID',
        source: 'xl-release',
        deployables: [
          { name: 'billing-api', host: 'srv-12.prod', package: 'billing-api/2.7.3' },
        ],
        pipeline_dependencies: [{ from: 'billing-db', to: 'billing-api' }],
      },
      null,
      2,
    ),
    csv: 'name,component_type,host,package,environment\nbilling-api,service,srv-12.prod,billing-api/2.7.3,prod\n\nfrom,to\nbilling-db,billing-api\n',
  },
  flows: {
    json: JSON.stringify(
      {
        application_id: 'PASTE-APPLICATION-UUID',
        source: 'flux-ref',
        flows: [
          { from: 'billing-api', to: 'billing-db', port: 5432, protocol: 'tcp' },
        ],
      },
      null,
      2,
    ),
    csv: 'from,to,port,protocol\nbilling-api,billing-db,5432,tcp\n',
  },
  incidents: {
    json: JSON.stringify(
      {
        organization_id: 'PASTE-ORG-UUID',
        application_id: 'PASTE-APPLICATION-UUID',
        source: 'servicenow',
        incidents: [
          {
            external_id: 'INC0012345',
            title: 'billing-api timeouts',
            severity: 'P1',
            status: 'resolved',
            opened_at: '2026-05-12T08:00:00Z',
            resolved_at: '2026-05-12T09:15:00Z',
            root_cause: 'JDBC pool saturated',
            impacted_component_names: ['billing-api'],
          },
        ],
      },
      null,
      2,
    ),
    csv: 'external_id,title,opened_at,resolved_at,severity,status,root_cause,impacted_components\nINC0012345,billing-api timeouts,2026-05-12T08:00:00Z,2026-05-12T09:15:00Z,P1,resolved,JDBC pool saturated,billing-api\n',
  },
};

interface IngestionResp {
  status: string;
  report: IngestionReport;
}

function useIngest() {
  return useMutation({
    mutationFn: async ({
      source,
      format,
      payload,
      applicationId,
      organizationId,
      sourceLabel,
    }: {
      source: Source;
      format: Format;
      payload: string;
      applicationId?: string;
      organizationId?: string;
      sourceLabel?: string;
    }) => {
      if (format === 'json') {
        const parsed = JSON.parse(payload);
        const res = await client.post<IngestionResp>(`/ingestion/${source}`, parsed);
        return res.data.report;
      }
      // CSV path — endpoints take query params and raw body
      const params: Record<string, string> = {};
      if (applicationId) params.application_id = applicationId;
      if (organizationId) params.organization_id = organizationId;
      if (sourceLabel) params.source = sourceLabel;
      const res = await client.post<IngestionResp>(
        `/ingestion/${source}/csv`,
        payload,
        {
          params,
          headers: { 'Content-Type': 'text/csv' },
        },
      );
      return res.data.report;
    },
  });
}

export function IngestionWizard({ className }: Props) {
  const [source, setSource] = useState<Source>('cmdb');
  const [format, setFormat] = useState<Format>('json');
  const [appId, setAppId] = useState<string>('');
  const [orgId, setOrgId] = useState<string>('');
  const [sourceLabel, setSourceLabel] = useState<string>('');
  const [payload, setPayload] = useState<string>(sourceTemplates.cmdb.json);

  const { data: apps } = useApps();
  const ingest = useIngest();

  const updateSource = (s: Source) => {
    setSource(s);
    setPayload(sourceTemplates[s][format]);
  };
  const updateFormat = (f: Format) => {
    setFormat(f);
    setPayload(sourceTemplates[source][f]);
  };

  const loadTemplate = () => setPayload(sourceTemplates[source][format]);

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    ingest.mutate({
      source,
      format,
      payload,
      applicationId: appId || undefined,
      organizationId: source === 'incidents' ? orgId || undefined : undefined,
      sourceLabel: sourceLabel || undefined,
    });
  };

  const report = ingest.data;

  return (
    <Card className={cn('border-l-4 border-l-teal-500', className)}>
      <CardHeader className="pb-2">
        <CardTitle className="flex items-center gap-2 text-sm">
          <Upload className="h-4 w-4 text-teal-600" />
          Wizard d'ingestion
        </CardTitle>
      </CardHeader>
      <CardContent>
        <form onSubmit={onSubmit} className="space-y-3">
          {/* Source selector */}
          <div>
            <label className="mb-1 block text-[10px] font-semibold uppercase tracking-wider text-slate-500">
              Source
            </label>
            <div className="flex flex-wrap gap-1">
              {(Object.keys(sourceLabels) as Source[]).map((s) => (
                <button
                  key={s}
                  type="button"
                  onClick={() => updateSource(s)}
                  className={cn(
                    'rounded-md border px-2 py-1 text-xs font-semibold',
                    source === s
                      ? 'border-teal-400 bg-teal-50 text-teal-800'
                      : 'border-slate-200 bg-white text-slate-600 hover:border-slate-400',
                  )}
                >
                  {sourceLabels[s]}
                </button>
              ))}
            </div>
          </div>

          {/* Format selector */}
          <div>
            <label className="mb-1 block text-[10px] font-semibold uppercase tracking-wider text-slate-500">
              Format
            </label>
            <div className="flex gap-1">
              <button
                type="button"
                onClick={() => updateFormat('json')}
                className={cn(
                  'flex flex-1 items-center justify-center gap-1 rounded-md border px-2 py-1 text-xs font-semibold',
                  format === 'json'
                    ? 'border-teal-400 bg-teal-50 text-teal-800'
                    : 'border-slate-200 bg-white text-slate-600',
                )}
              >
                <FileJson className="h-3 w-3" /> JSON
              </button>
              <button
                type="button"
                onClick={() => updateFormat('csv')}
                className={cn(
                  'flex flex-1 items-center justify-center gap-1 rounded-md border px-2 py-1 text-xs font-semibold',
                  format === 'csv'
                    ? 'border-teal-400 bg-teal-50 text-teal-800'
                    : 'border-slate-200 bg-white text-slate-600',
                )}
              >
                <FileSpreadsheet className="h-3 w-3" /> CSV
              </button>
            </div>
          </div>

          {/* Target app (CSV only — JSON carries it in the payload) */}
          {format === 'csv' && (
            <div className="grid gap-2 md:grid-cols-2">
              <div>
                <label className="mb-1 block text-[10px] font-semibold uppercase tracking-wider text-slate-500">
                  Application cible
                </label>
                <select
                  value={appId}
                  onChange={(e) => setAppId(e.target.value)}
                  className="w-full rounded-md border border-slate-200 bg-white px-2 py-1 text-xs"
                >
                  <option value="">— Choisir une application —</option>
                  {apps?.apps?.map((a: { id: string; name: string }) => (
                    <option key={a.id} value={a.id}>
                      {a.name}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className="mb-1 block text-[10px] font-semibold uppercase tracking-wider text-slate-500">
                  Étiquette source (optionnel)
                </label>
                <input
                  type="text"
                  value={sourceLabel}
                  onChange={(e) => setSourceLabel(e.target.value)}
                  placeholder={`${source}-csv`}
                  className="w-full rounded-md border border-slate-200 bg-white px-2 py-1 text-xs"
                />
              </div>
              {source === 'incidents' && (
                <div className="md:col-span-2">
                  <label className="mb-1 block text-[10px] font-semibold uppercase tracking-wider text-slate-500">
                    Organization ID (incidents)
                  </label>
                  <input
                    type="text"
                    value={orgId}
                    onChange={(e) => setOrgId(e.target.value)}
                    placeholder="UUID de l'organisation"
                    className="w-full rounded-md border border-slate-200 bg-white px-2 py-1 font-mono text-xs"
                  />
                </div>
              )}
            </div>
          )}

          {/* Payload editor */}
          <div>
            <label className="mb-1 flex items-center justify-between text-[10px] font-semibold uppercase tracking-wider text-slate-500">
              <span>Payload {format.toUpperCase()}</span>
              <button
                type="button"
                onClick={loadTemplate}
                className="text-[10px] font-normal text-teal-700 hover:text-teal-900"
              >
                ↺ recharger le modèle
              </button>
            </label>
            <textarea
              value={payload}
              onChange={(e) => setPayload(e.target.value)}
              rows={8}
              spellCheck={false}
              className="w-full resize-y rounded-md border border-slate-200 bg-slate-50 p-2 font-mono text-[11px] leading-snug focus:border-teal-500 focus:outline-none"
            />
          </div>

          <div className="flex items-center justify-between">
            <p className="text-[11px] text-slate-500">
              Endpoint : <span className="font-mono">POST /api/v1/ingestion/{source}{format === 'csv' ? '/csv' : ''}</span>
            </p>
            <Button type="submit" disabled={ingest.isPending}>
              {ingest.isPending ? (
                <>
                  <Loader2 className="h-3 w-3 animate-spin" /> Ingestion…
                </>
              ) : (
                'Ingérer'
              )}
            </Button>
          </div>
        </form>

        {ingest.isError && (
          <div className="mt-3 flex items-start gap-2 rounded-md border border-red-200 bg-red-50 p-3 text-xs text-red-800">
            <AlertTriangle className="h-4 w-4 shrink-0" />
            <span>{(ingest.error as Error)?.message ?? 'Erreur lors de l\'ingestion'}</span>
          </div>
        )}

        {report && (
          <div className="mt-3 rounded-md border border-emerald-200 bg-emerald-50 p-3 text-xs text-emerald-900">
            <div className="mb-2 flex items-center gap-2 font-semibold">
              <CheckCircle className="h-4 w-4" />
              Ingestion réussie · source <span className="font-mono">{report.source}</span>
            </div>
            <div className="flex flex-wrap gap-2">
              <Badge variant="outline" className="border-emerald-400 bg-white">
                {report.created} créé{report.created > 1 ? 's' : ''}
              </Badge>
              <Badge variant="outline" className="border-indigo-400 bg-white">
                {report.updated} mis à jour
              </Badge>
              <Badge variant="outline" className="border-slate-300 bg-white">
                {report.skipped} ignoré{report.skipped > 1 ? 's' : ''}
              </Badge>
              {report.errors.length > 0 && (
                <Badge variant="outline" className="border-red-400 bg-white text-red-800">
                  {report.errors.length} erreur{report.errors.length > 1 ? 's' : ''}
                </Badge>
              )}
            </div>
            {report.errors.length > 0 && (
              <ul className="mt-2 space-y-1 text-[11px]">
                {report.errors.slice(0, 5).map((e, i) => (
                  <li key={i} className="font-mono">
                    {e.item ? `${e.item} → ` : ''}
                    {e.message}
                  </li>
                ))}
                {report.errors.length > 5 && (
                  <li className="text-slate-500">
                    … et {report.errors.length - 5} autres
                  </li>
                )}
              </ul>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// Re-export the typed report so callers can render their own dashboards.
export type { IngestionReport };
