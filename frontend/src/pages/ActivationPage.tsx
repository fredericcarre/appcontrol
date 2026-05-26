import { useParams } from 'react-router-dom';
import { useActivation, useSetActivation, ActivationStatus } from '@/api/activation';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { ActivationBadge } from '@/components/ActivationBadge';

/**
 * Activation page — one application at a time.
 *
 * Lists the five levels of the graduated adoption ladder and lets a
 * Manage-permission user move the application up or down. Mirrors the
 * vocabulary used in the strategy / methodology / vision documents so a
 * reader of those docs immediately recognises what each level means.
 */
export function ActivationPage() {
  const { id: appId } = useParams<{ id: string }>();
  const { data, isLoading, error } = useActivation(appId);
  const setLevel = useSetActivation(appId);

  if (isLoading) {
    return (
      <div className="p-8 text-sm text-muted-foreground">
        Chargement du niveau d'activation…
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className="p-8">
        <Card className="border-red-200 bg-red-50">
          <CardContent className="p-4 text-sm text-red-800">
            Impossible de charger le niveau d'activation
            {error instanceof Error ? `: ${error.message}` : ''}.
          </CardContent>
        </Card>
      </div>
    );
  }

  const current = data.activation;

  return (
    <div className="mx-auto max-w-4xl space-y-6 p-8">
      <header>
        <p className="text-xs font-semibold uppercase tracking-widest text-teal-700">
          Adoption graduelle
        </p>
        <h1 className="mt-1 text-2xl font-bold text-slate-900">
          Niveau d'activation de l'application
        </h1>
        <p className="mt-2 max-w-2xl text-sm text-slate-600">
          La trajectoire d'<em>efficience opérationnelle</em> se pilote palier
          par palier. Plus le niveau est haut, plus AppControl est autorisé à
          agir sur l'application. Chaque palier est <strong>réversible</strong>{' '}
          et garde son audit log natif.
        </p>
      </header>

      <Card className="border-2 border-teal-500">
        <CardHeader>
          <CardTitle className="flex items-center justify-between">
            <span>Niveau actuel</span>
            <ActivationBadge status={current} />
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-slate-700">{current.description}</p>
          <div className="mt-4 flex flex-wrap gap-2">
            <Capability ok={current.allows_checks} label="Checks 3 niveaux actifs" />
            <Capability ok={current.allows_ops} label="Opérations (start / stop / rebuild)" />
            <Capability
              ok={current.requires_pr_approval}
              label="PR mergée requise"
              neutral
            />
          </div>
        </CardContent>
      </Card>

      <div className="grid gap-3">
        {data.available_levels.map((lvl) => (
          <LevelCard
            key={lvl.level}
            level={lvl}
            current={current}
            onSelect={() => setLevel.mutate(lvl.level)}
            disabled={setLevel.isPending}
          />
        ))}
      </div>

      {setLevel.isError && (
        <Card className="border-red-200 bg-red-50">
          <CardContent className="p-4 text-sm text-red-800">
            Mise à jour refusée. Vérifie que tu as la permission <em>manage</em>{' '}
            sur cette application.
          </CardContent>
        </Card>
      )}
    </div>
  );
}

function LevelCard({
  level,
  current,
  onSelect,
  disabled,
}: {
  level: ActivationStatus;
  current: ActivationStatus;
  onSelect: () => void;
  disabled: boolean;
}) {
  const isCurrent = level.level === current.level;
  const isUpgrade = level.level > current.level;
  const isDowngrade = level.level < current.level;

  return (
    <Card className={isCurrent ? 'border-teal-400' : 'border-slate-200'}>
      <CardContent className="p-4">
        <div className="flex items-start justify-between gap-4">
          <div className="flex-1">
            <div className="flex items-center gap-3">
              <span className="text-xs font-bold uppercase tracking-widest text-slate-500">
                Niveau {level.level}
              </span>
              <ActivationBadge status={level} />
              {isCurrent && (
                <Badge variant="outline" className="text-[10px]">
                  Actuel
                </Badge>
              )}
            </div>
            <p className="mt-2 text-sm text-slate-700">{level.description}</p>
          </div>
          {!isCurrent && (
            <Button
              size="sm"
              variant={isUpgrade ? 'default' : 'outline'}
              onClick={onSelect}
              disabled={disabled}
            >
              {isUpgrade ? 'Passer au niveau ' : 'Redescendre au niveau '}
              {level.level}
            </Button>
          )}
          {isDowngrade && null}
        </div>
      </CardContent>
    </Card>
  );
}

function Capability({
  ok,
  label,
  neutral,
}: {
  ok: boolean;
  label: string;
  neutral?: boolean;
}) {
  if (neutral) {
    return ok ? (
      <Badge variant="outline" className="border-amber-400 bg-amber-50 text-amber-800">
        ⚠ {label}
      </Badge>
    ) : null;
  }
  return (
    <Badge
      variant="outline"
      className={
        ok
          ? 'border-emerald-300 bg-emerald-50 text-emerald-800'
          : 'border-slate-300 bg-slate-50 text-slate-500 line-through'
      }
    >
      {ok ? '✓ ' : '— '}
      {label}
    </Badge>
  );
}
