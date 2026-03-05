import { useLocation, Link } from 'react-router-dom';
import { ChevronRight, Home } from 'lucide-react';
import { useApp } from '@/api/apps';

const routeLabels: Record<string, string> = {
  '': 'Dashboard',
  'teams': 'Teams',
  'agents': 'Agents',
  'gateways': 'Gateways',
  'reports': 'Reports',
  'settings': 'Settings',
  'onboarding': 'Onboarding',
  'apps': 'Applications',
  'import': 'Import',
  'discovery': 'Discovery',
};

// Check if a string looks like a UUID
function isUuid(str: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(str);
}

export function Breadcrumbs() {
  const location = useLocation();
  const segments = location.pathname.split('/').filter(Boolean);

  // Check if we're on an app page (e.g., /apps/{uuid})
  const appIdIndex = segments.findIndex((s) => s === 'apps') + 1;
  const appId = appIdIndex > 0 && appIdIndex < segments.length ? segments[appIdIndex] : null;
  const isAppId = appId && isUuid(appId);

  // Fetch app data if we have an app ID
  const { data: app } = useApp(isAppId ? appId : '');

  return (
    <nav className="flex items-center gap-1 text-sm text-muted-foreground">
      <Link to="/" className="hover:text-foreground transition-colors">
        <Home className="h-4 w-4" />
      </Link>
      {segments.map((seg, i) => {
        const path = '/' + segments.slice(0, i + 1).join('/');

        // Use app name if this segment is the app ID
        let label = routeLabels[seg] || seg;
        if (isAppId && seg === appId && app?.name) {
          label = app.name;
        }

        const isLast = i === segments.length - 1;

        return (
          <span key={path} className="flex items-center gap-1">
            <ChevronRight className="h-3 w-3" />
            {isLast ? (
              <span className="text-foreground font-medium">{label}</span>
            ) : (
              <Link to={path} className="hover:text-foreground transition-colors">
                {label}
              </Link>
            )}
          </span>
        );
      })}
    </nav>
  );
}
