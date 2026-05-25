import { useState } from 'react';
import { Database, GitPullRequestArrow, Workflow, AlertTriangle, Brain, Library } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { usePatterns, usePatternCandidates, usePropagatePattern } from '@/api/patterns';
import client from '@/api/client';
import { useMutation } from '@tanstack/react-query';
import { cn } from '@/lib/utils';

/**
 * Captation hub — the methodology's Phase 1 (multi-source ingestion)
 * and Phase 5 (transversal capitalisation) live here. The page is
 * intentionally one screen, organised in three vertical bands:
 *
 *   1. Sources               — what feeds the maps (CMDB, XLR/XLD,
 *                              flows, incidents, ServiceNow, Jira)
 *   2. Pattern library       — reusable command templates per
 *                              technology, with cross-application
 *                              propagation
 *   3. AI corpus & RAG       — runbook indexation status, query box
 *
 * The user sees, in one place, what AppControl can learn from and
 * how that learning is propagated.
 */
export function CaptationPage() {
  return (
    <div className="mx-auto max-w-6xl space-y-8 p-8">
      <header>
        <p className="text-xs font-semibold uppercase tracking-widest text-teal-700">
          Méthodologie · Phases 1 &amp; 5
        </p>
        <h1 className="mt-1 text-2xl font-bold text-slate-900">
          Captation &amp; capitalisation
        </h1>
        <p className="mt-2 max-w-3xl text-sm text-slate-600">
          AppControl agrège ce que l'entreprise sait déjà — CMDB, outils de
          déploiement, référentiels de flux, incidents ITSM — puis capitalise
          chaque apprentissage opérationnel dans une bibliothèque transverse
          de patterns qui se propage aux applications similaires.
        </p>
      </header>

      <SourcesSection />
      <PatternsSection />
      <RagSection />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sources — ingestion endpoints by methodology family
// ---------------------------------------------------------------------------

function SourcesSection() {
  return (
    <section>
      <h2 className="mb-3 flex items-center gap-2 text-sm font-bold uppercase tracking-wider text-slate-700">
        <Database className="h-4 w-4 text-teal-600" />
        Sources d'ingestion
      </h2>
      <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
        <SourceCard
          title="CMDB"
          icon={<Database className="h-4 w-4" />}
          description="Composants, briques techniques, propriétaires."
          endpoints={['POST /ingestion/cmdb', 'POST /ingestion/cmdb/csv']}
          accent="teal"
        />
        <SourceCard
          title="XL Release / XL Deploy"
          icon={<Workflow className="h-4 w-4" />}
          description="Pipelines, manifests, dépendances de déploiement."
          endpoints={['POST /ingestion/xl', 'POST /ingestion/xl/csv']}
          accent="teal"
        />
        <SourceCard
          title="Référentiel de flux"
          icon={<Workflow className="h-4 w-4" />}
          description="Liens réseau autorisés, ports, protocoles."
          endpoints={['POST /ingestion/flows', 'POST /ingestion/flows/csv']}
          accent="teal"
        />
        <SourceCard
          title="ITSM / incidents"
          icon={<AlertTriangle className="h-4 w-4" />}
          description="Tickets historiques, sévérité, composants impactés."
          endpoints={['POST /ingestion/incidents', 'POST /ingestion/incidents/csv']}
          accent="amber"
        />
        <SourceCard
          title="ServiceNow (pull)"
          icon={<GitPullRequestArrow className="h-4 w-4" />}
          description="Récupération native depuis l'API Table ServiceNow."
          endpoints={['POST /ingestion/pull/servicenow']}
          accent="indigo"
        />
        <SourceCard
          title="Jira Service Management (pull)"
          icon={<GitPullRequestArrow className="h-4 w-4" />}
          description="Récupération par requête JQL."
          endpoints={['POST /ingestion/pull/jira']}
          accent="indigo"
        />
      </div>
    </section>
  );
}

function SourceCard({
  title,
  icon,
  description,
  endpoints,
  accent,
}: {
  title: string;
  icon: React.ReactNode;
  description: string;
  endpoints: string[];
  accent: 'teal' | 'indigo' | 'amber';
}) {
  const palette = {
    teal: 'border-l-teal-500',
    indigo: 'border-l-indigo-500',
    amber: 'border-l-amber-500',
  }[accent];
  return (
    <Card className={cn('border-l-4', palette)}>
      <CardHeader className="pb-2">
        <CardTitle className="flex items-center gap-2 text-sm">
          {icon}
          {title}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-2">
        <p className="text-xs text-slate-600">{description}</p>
        <ul className="space-y-1">
          {endpoints.map((e) => (
            <li
              key={e}
              className="rounded bg-slate-100 px-2 py-1 font-mono text-[11px] text-slate-700"
            >
              {e}
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Patterns — transversal capitalisation
// ---------------------------------------------------------------------------

function PatternsSection() {
  const { data, isLoading } = usePatterns();
  const [selected, setSelected] = useState<string | null>(null);

  return (
    <section>
      <h2 className="mb-3 flex items-center gap-2 text-sm font-bold uppercase tracking-wider text-slate-700">
        <Library className="h-4 w-4 text-indigo-600" />
        Bibliothèque de patterns
      </h2>
      <p className="mb-3 text-xs text-slate-600">
        Chaque pattern capitalise une bonne pratique par technologie (checks,
        commandes, rebuild). Les patterns nourrissent les composants
        similaires via la propagation.
      </p>

      {isLoading && <p className="text-xs text-slate-500">Chargement…</p>}
      {data?.patterns.length === 0 && (
        <Card>
          <CardContent className="p-4 text-sm text-slate-600">
            Aucun pattern pour le moment. Crée le premier depuis un incident
            résolu (POST <code className="rounded bg-slate-100 px-1">/patterns</code>),
            ou attends qu'un drill remonte une recommandation.
          </CardContent>
        </Card>
      )}

      <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
        {data?.patterns.map((p) => (
          <PatternCard
            key={p.id}
            id={p.id}
            name={p.name}
            technology={p.technology}
            description={p.description ?? ''}
            usageCount={p.usage_count}
            createdFromIncident={!!p.created_from_incident_id}
            onSelect={() => setSelected((s) => (s === p.id ? null : p.id))}
            isSelected={selected === p.id}
          />
        ))}
      </div>

      {selected && (
        <div className="mt-4">
          <PatternCandidatesPanel patternId={selected} />
        </div>
      )}
    </section>
  );
}

function PatternCard({
  id,
  name,
  technology,
  description,
  usageCount,
  createdFromIncident,
  onSelect,
  isSelected,
}: {
  id: string;
  name: string;
  technology: string;
  description: string;
  usageCount: number;
  createdFromIncident: boolean;
  onSelect: () => void;
  isSelected: boolean;
}) {
  return (
    <Card
      role="button"
      onClick={onSelect}
      className={cn(
        'cursor-pointer transition',
        isSelected
          ? 'border-indigo-400 ring-2 ring-indigo-200'
          : 'hover:border-indigo-300',
      )}
    >
      <CardHeader className="pb-2">
        <CardTitle className="flex items-center justify-between text-sm">
          <span className="truncate">{name}</span>
          <Badge
            variant="outline"
            className="border-indigo-300 bg-indigo-50 text-[10px] text-indigo-800"
          >
            {technology}
          </Badge>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-2">
        <p className="line-clamp-2 text-xs text-slate-600">
          {description || <span className="italic text-slate-400">(pas de description)</span>}
        </p>
        <div className="flex items-center gap-2">
          <Badge variant="outline" className="text-[10px] font-mono">
            usage {usageCount}
          </Badge>
          {createdFromIncident && (
            <Badge
              variant="outline"
              className="border-amber-300 bg-amber-50 text-[10px] text-amber-800"
              title="Pattern dérivé d'un incident — Phase 5 du cycle d'apprentissage"
            >
              issu d'un incident
            </Badge>
          )}
        </div>
        <p className="text-[10px] uppercase tracking-wider text-slate-500">
          {isSelected ? 'Cliquer pour replier' : 'Cliquer pour voir les candidats à la propagation'}
        </p>
        <input type="hidden" value={id} />
      </CardContent>
    </Card>
  );
}

function PatternCandidatesPanel({ patternId }: { patternId: string }) {
  const { data, isLoading } = usePatternCandidates(patternId);
  const propagate = usePropagatePattern();
  const [selected, setSelected] = useState<Record<string, boolean>>({});

  const candidates = data?.candidates ?? [];
  const selectedIds = candidates
    .filter((c) => selected[c.component_id])
    .map((c) => c.component_id);

  if (isLoading) {
    return <p className="text-xs text-slate-500">Recherche des composants candidats…</p>;
  }

  if (candidates.length === 0) {
    return (
      <Card>
        <CardContent className="p-4 text-sm text-slate-600">
          Aucun candidat à la propagation. Soit toutes les applications éligibles
          appliquent déjà ce pattern, soit aucune n'utilise cette technologie.
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="flex items-center justify-between text-sm">
          <span>Candidats à la propagation</span>
          <Button
            size="sm"
            disabled={selectedIds.length === 0 || propagate.isPending}
            onClick={() =>
              propagate.mutate({ id: patternId, componentIds: selectedIds })
            }
          >
            Appliquer à {selectedIds.length} composant{selectedIds.length > 1 ? 's' : ''}
          </Button>
        </CardTitle>
      </CardHeader>
      <CardContent>
        <ul className="space-y-1">
          {candidates.map((c) => (
            <li
              key={c.component_id}
              className="flex items-center gap-2 rounded border border-slate-200 bg-white px-3 py-2 text-xs"
            >
              <input
                type="checkbox"
                checked={!!selected[c.component_id]}
                onChange={(e) =>
                  setSelected((prev) => ({
                    ...prev,
                    [c.component_id]: e.target.checked,
                  }))
                }
                className="accent-teal-600"
              />
              <span className="font-mono text-slate-500">{c.application_name}</span>
              <span className="text-slate-400">/</span>
              <span className="flex-1 truncate font-semibold text-slate-800">
                {c.component_name}
              </span>
              <Badge variant="outline" className="text-[10px]">
                {c.component_type}
              </Badge>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// RAG corpus & query box
// ---------------------------------------------------------------------------

interface RagAnswer {
  query: string;
  corpus_dir: string;
  chunk_count: number;
  matches: Array<{
    source: string;
    chunk_index: number;
    text: string;
    score: number;
  }>;
}

function useRagQuery() {
  return useMutation({
    mutationFn: async (input: { query: string; topK: number }) => {
      const res = await client.post<{ status: string; response: RagAnswer }>(
        '/ai/rag/query',
        { query: input.query, top_k: input.topK },
      );
      return res.data.response;
    },
  });
}

function RagSection() {
  const [query, setQuery] = useState('');
  const ragQuery = useRagQuery();

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!query.trim()) return;
    ragQuery.mutate({ query: query.trim(), topK: 5 });
  };

  return (
    <section>
      <h2 className="mb-3 flex items-center gap-2 text-sm font-bold uppercase tracking-wider text-slate-700">
        <Brain className="h-4 w-4 text-emerald-600" />
        Corpus IA &amp; RAG
      </h2>
      <p className="mb-3 text-xs text-slate-600">
        AppControl indexe le corpus de runbooks pointé par
        <code className="mx-1 rounded bg-slate-100 px-1 font-mono text-[11px]">
          RAG_CORPUS_DIR
        </code>
        et alimente les réponses IA contextuelles (analyse causale,
        suggestions). Tu peux tester l'index ici.
      </p>

      <Card>
        <CardContent className="space-y-3 p-4">
          <form onSubmit={onSubmit} className="flex gap-2">
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Pose une question — par ex. : 'pool JDBC Spring Boot saturé'"
              className="flex-1 rounded-md border border-slate-200 bg-white px-3 py-2 text-sm focus:border-emerald-500 focus:outline-none"
            />
            <Button type="submit" disabled={!query.trim() || ragQuery.isPending}>
              Interroger
            </Button>
          </form>

          {ragQuery.isError && (
            <p className="text-xs text-red-600">
              {(ragQuery.error as Error)?.message ?? 'Erreur'}.
              Vérifie que <code className="font-mono">RAG_CORPUS_DIR</code> est configuré côté backend.
            </p>
          )}

          {ragQuery.data && (
            <div className="space-y-2">
              <p className="text-[10px] uppercase tracking-wider text-slate-500">
                {ragQuery.data.chunk_count} chunks indexés depuis{' '}
                <span className="font-mono">{ragQuery.data.corpus_dir}</span> —{' '}
                {ragQuery.data.matches.length} résultat(s)
              </p>
              {ragQuery.data.matches.map((m) => (
                <div
                  key={`${m.source}-${m.chunk_index}`}
                  className="rounded-md border border-slate-200 bg-white p-3"
                >
                  <div className="mb-1 flex items-center justify-between">
                    <span className="font-mono text-[11px] text-slate-500">
                      {m.source} · chunk #{m.chunk_index}
                    </span>
                    <Badge variant="outline" className="text-[10px]">
                      score {m.score.toFixed(2)}
                    </Badge>
                  </div>
                  <p className="line-clamp-4 whitespace-pre-wrap text-xs text-slate-700">
                    {m.text}
                  </p>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </section>
  );
}
