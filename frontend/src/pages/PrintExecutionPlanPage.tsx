import { useEffect, useMemo, useRef } from 'react';
import { useParams } from 'react-router-dom';
import { useApp, type Component, type Dependency } from '@/api/apps';

/**
 * Printable execution plan for an application — opened in a new tab from
 * the map's "Print plan" button. Designed to be the source for a PDF the
 * operator generates via the browser (Ctrl+P → "Save as PDF").
 *
 * The page lays out:
 *   * App identification + a dated header (so a saved PDF self-describes).
 *   * The DAG levels in topological order, each containing the components
 *     that start in parallel.
 *   * Per component: name, type, host/agent, expected behaviour, the
 *     start_cmd / stop_cmd to run, and the check_cmd or native probe to
 *     confirm health.
 *
 * The `@media print` CSS hides the toolbar and forces page breaks between
 * levels so the PDF reads as a runbook even at high component counts.
 *
 * No new backend endpoint: everything we need is already in /apps/:id.
 * This keeps the feature reversible — operators who don't generate PDFs
 * never see a page nobody looks at.
 */
export default function PrintExecutionPlanPage() {
  const { appId } = useParams<{ appId: string }>();
  const { data: app, isLoading } = useApp(appId || '');

  const components = app?.components ?? [];
  const dependencies = app?.dependencies ?? [];

  const levels = useMemo(
    () => topologicalLevels(components, dependencies),
    [components, dependencies],
  );

  // Auto-open the print dialog once per page load, after the data is in
  // and React has finished painting. Using a ref so re-renders (e.g. a
  // dependency tick) don't re-trigger the dialog while the operator is
  // already interacting with it. The operator can also click the
  // "Print / Save as PDF" toolbar button to fire it manually.
  const autoFiredRef = useRef(false);
  useEffect(() => {
    if (autoFiredRef.current) return;
    if (!isLoading && components.length > 0) {
      autoFiredRef.current = true;
      // Two animation frames give the browser time to lay out the full
      // document before the print engine snapshots it — without this
      // delay we've seen the dialog open against a half-rendered DOM
      // and report "1 page total" because only the header had painted.
      const id = window.requestAnimationFrame(() =>
        window.requestAnimationFrame(() => window.print()),
      );
      return () => window.cancelAnimationFrame(id);
    }
  }, [isLoading, components.length]);

  if (isLoading) return <div className="p-8">Loading…</div>;
  if (!app) return <div className="p-8">App not found</div>;

  const generated = new Date().toLocaleString();

  return (
    <div className="print-plan-root">
      <style>{PRINT_CSS}</style>

      {/* Toolbar — hidden in print */}
      <div className="no-print sticky top-0 bg-white border-b px-4 py-2 flex items-center justify-between shadow-sm">
        <h1 className="text-base font-semibold">
          {app.name} — Execution Plan (preview)
        </h1>
        <div className="flex gap-2">
          <button
            onClick={() => window.print()}
            className="px-3 py-1.5 rounded bg-blue-600 text-white text-sm hover:bg-blue-700"
          >
            Print / Save as PDF
          </button>
          <button
            onClick={() => window.close()}
            className="px-3 py-1.5 rounded border text-sm hover:bg-gray-50"
          >
            Close
          </button>
        </div>
      </div>

      {/* Document body — width tracks the viewport so a 200-component plan
          uses the page instead of a tall scrollable column. The print
          stylesheet relaxes the padding to A4 margins. */}
      <article className="plan plan-body mx-auto p-8" style={{ maxWidth: '210mm' }}>
        <header className="mb-6 border-b pb-4">
          <div className="text-xs text-gray-500 uppercase tracking-wider">
            AppControl execution plan
          </div>
          <h1 className="text-2xl font-bold mt-1">{app.name}</h1>
          {app.description && (
            <p className="text-sm text-gray-700 mt-1">{app.description}</p>
          )}
          <dl className="text-xs text-gray-600 mt-3 grid grid-cols-2 gap-x-4 gap-y-1">
            <dt className="font-semibold">Application ID</dt>
            <dd className="font-mono">{app.id}</dd>
            <dt className="font-semibold">Components</dt>
            <dd>{components.length}</dd>
            <dt className="font-semibold">DAG levels</dt>
            <dd>{levels.length}</dd>
            <dt className="font-semibold">Generated</dt>
            <dd>{generated}</dd>
          </dl>
        </header>

        <section className="mb-4">
          <h2 className="text-lg font-semibold mb-2">How to read this plan</h2>
          <ul className="text-sm text-gray-800 list-disc pl-5 space-y-1">
            <li>
              Components are grouped into <strong>levels</strong>. Within a
              level, components have no inter-dependency and start in
              parallel.
            </li>
            <li>
              Levels run in order: every component in level <em>N</em> must
              be healthy before any component in level <em>N+1</em> starts.
            </li>
            <li>
              Run the <strong>start command</strong>, then verify with the
              <strong> check command</strong>. A health check returning
              exit code 0 is the success criterion.
            </li>
          </ul>
        </section>

        {levels.length === 0 && (
          <p className="text-gray-600">
            No startable components — add at least one component with a
            start command.
          </p>
        )}

        {levels.map((level, idx) => (
          <section key={idx} className="level-section">
            <h2 className="level-heading text-lg font-semibold mt-6 mb-2 border-b pb-1">
              Level {idx + 1}{' '}
              <span className="text-sm font-normal text-gray-500">
                ({level.length} component{level.length !== 1 ? 's' : ''} in
                parallel)
              </span>
            </h2>
            {level.map((comp) => (
              <ComponentBlock key={comp.id} comp={comp} />
            ))}
          </section>
        ))}

        <section className="rollback mt-8 border-t pt-4">
          <h2 className="text-lg font-semibold mb-2">Rollback / stop order</h2>
          <p className="text-sm text-gray-700 mb-2">
            To stop the application cleanly, walk the levels in
            <strong> reverse</strong>: stop everything in level{' '}
            {levels.length} first, then level {Math.max(levels.length - 1, 1)},
            and so on.
          </p>
          <ol className="text-sm list-decimal pl-5 space-y-0.5">
            {[...levels].reverse().map((level, idx) => (
              <li key={idx}>
                Level {levels.length - idx}:{' '}
                {level.map((c) => c.display_name || c.name).join(', ')}
              </li>
            ))}
          </ol>
        </section>

        <footer className="text-[10px] text-gray-400 text-center mt-10 border-t pt-3">
          Generated from AppControl — {generated} — Application {app.id}
        </footer>
      </article>
    </div>
  );
}

function ComponentBlock({ comp }: { comp: Component }) {
  return (
    <div className="component-block break-inside-avoid border rounded p-3 mb-3 text-sm">
      <div className="flex items-baseline justify-between flex-wrap gap-1">
        <h3 className="font-semibold">
          {comp.display_name || comp.name}
          {comp.display_name && (
            <span className="text-xs text-gray-500 font-normal ml-1">
              ({comp.name})
            </span>
          )}
        </h3>
        <div className="text-xs text-gray-500 space-x-2">
          <span>type: {comp.component_type}</span>
          {comp.host && <span>· host: {comp.host}</span>}
          {comp.cluster_mode === 'fan_out' && <span>· fan-out</span>}
        </div>
      </div>

      {comp.cluster_mode === 'fan_out' && (
        <div className="text-xs text-gray-600 mt-1">
          Fan-out cluster — start dispatched to every enabled member;
          aggregate state from <code>{comp.cluster_health_policy ?? 'all_healthy'}</code>
          {comp.cluster_min_healthy_pct
            ? ` (≥${comp.cluster_min_healthy_pct}%)`
            : ''}
          .
        </div>
      )}

      <CommandRow label="Start command" value={comp.start_cmd} />
      <CommandRow label="Health check" value={comp.check_cmd} />
      <CommandRow label="Stop command" value={comp.stop_cmd} mono small />

      <div className="text-xs text-gray-700 mt-2">
        <strong>How to test:</strong> run the health check above. A 0 exit
        code means the component is RUNNING. Any non-zero exit code, or a
        timeout greater than {comp.start_timeout_seconds}s after the start
        command was issued, indicates the component is FAILED.
      </div>
    </div>
  );
}

function CommandRow({
  label,
  value,
  small,
  mono = true,
}: {
  label: string;
  value?: string | null;
  small?: boolean;
  mono?: boolean;
}) {
  if (!value) {
    return (
      <div className={`mt-2 ${small ? 'text-[11px]' : 'text-xs'}`}>
        <strong>{label}:</strong>{' '}
        <span className="italic text-gray-500">(none defined)</span>
      </div>
    );
  }
  return (
    <div className={`mt-2 ${small ? 'text-[11px]' : 'text-xs'}`}>
      <strong>{label}:</strong>
      <pre
        className={`${mono ? 'font-mono' : ''} bg-gray-50 border rounded px-2 py-1 mt-1 whitespace-pre-wrap break-words`}
      >
        {value}
      </pre>
    </div>
  );
}

/**
 * Kahn's algorithm: build levels of components that can start in parallel.
 * Edge convention: a `from -> to` dependency means `from` depends on `to`,
 * so `to` must start first. Level 0 = components with no dependencies they
 * point to. Same convention as `core::sequencer::build_start_plan`.
 */
function topologicalLevels(
  components: Component[],
  dependencies: Dependency[],
): Component[][] {
  const byId = new Map(components.map((c) => [c.id, c]));
  // outDegree[id] = number of unsatisfied dependencies this component has
  const outDegree = new Map<string, number>();
  // reverseAdj[id] = components that depend ON this one (downstream)
  const reverseAdj = new Map<string, string[]>();
  for (const c of components) {
    outDegree.set(c.id, 0);
    reverseAdj.set(c.id, []);
  }
  for (const d of dependencies) {
    if (!byId.has(d.from_component_id) || !byId.has(d.to_component_id)) continue;
    outDegree.set(d.from_component_id, (outDegree.get(d.from_component_id) ?? 0) + 1);
    reverseAdj.get(d.to_component_id)!.push(d.from_component_id);
  }

  const levels: Component[][] = [];
  let frontier = components.filter((c) => (outDegree.get(c.id) ?? 0) === 0);
  while (frontier.length > 0) {
    levels.push(frontier);
    const next: Component[] = [];
    for (const c of frontier) {
      for (const dependent of reverseAdj.get(c.id) ?? []) {
        const remaining = (outDegree.get(dependent) ?? 0) - 1;
        outDegree.set(dependent, remaining);
        if (remaining === 0) next.push(byId.get(dependent)!);
      }
    }
    frontier = next;
  }
  return levels;
}

// Print stylesheet for the execution plan page.
//
// Some defensive rules learned from operator reports:
//   * `html, body, #root, .print-plan-root` must all reset `height` and
//     `overflow` in print — without this, Tailwind's preflight or the
//     SPA router root can cap the rendered document at viewport height,
//     producing a single A4 page with only the header visible and the
//     rest of the levels mysteriously missing.
//   * The toolbar and any global SPA chrome are removed via `.no-print`.
//   * `.plan` drops its inline `max-width: 210mm` and its `mx-auto`/
//     `p-8` so the body fills the @page margins instead of being
//     centered with extra padding.
//   * `.level-section` does NOT have `page-break-inside: avoid` (a
//     past version did, and a level with many components ran off the
//     page instead of paginating — operators saw "a scrollbar instead
//     of multiple pages"). Only individual `.component-block` rows
//     are protected from mid-card splits; levels can span pages.
//   * `page-break-after: avoid` on the heading keeps a level heading
//     glued to its first component card.
const PRINT_CSS = `
@media print {
  html, body {
    background: white !important;
    height: auto !important;
    min-height: 0 !important;
    overflow: visible !important;
  }
  #root, .print-plan-root {
    height: auto !important;
    min-height: 0 !important;
    overflow: visible !important;
    display: block !important;
  }
  .no-print { display: none !important; }
  .plan, .plan-body {
    max-width: none !important;
    width: auto !important;
    margin: 0 !important;
    padding: 0 !important;
    box-shadow: none !important;
  }
  .level-section { display: block !important; }
  .level-heading { page-break-after: avoid; break-after: avoid; }
  .component-block {
    page-break-inside: avoid;
    break-inside: avoid;
    box-shadow: none !important;
  }
  pre {
    font-size: 10px;
    white-space: pre-wrap !important;
    word-break: break-word !important;
  }
}

@page {
  size: A4;
  margin: 1cm 1.5cm;
}
`;
