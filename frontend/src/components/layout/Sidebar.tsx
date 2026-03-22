import { NavLink } from 'react-router-dom';
import { useUiStore } from '@/stores/ui';
import { useAuthStore } from '@/stores/auth';
import { cn } from '@/lib/utils';
import {
  LayoutDashboard,
  Users,
  UserCog,
  Server,
  Network,
  BarChart3,
  Settings,
  ChevronLeft,
  ChevronRight,
  Shield,
  Upload,
  KeyRound,
  Radar,
  Monitor,
  MapPin,
} from 'lucide-react';

const navItems = [
  { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
  { to: '/supervision', icon: Monitor, label: 'Supervision' },
  { to: '/discovery', icon: Radar, label: 'Discovery' },
  { to: '/teams', icon: Users, label: 'Teams' },
  { to: '/users', icon: UserCog, label: 'Users', adminOnly: true },
  { to: '/sites', icon: MapPin, label: 'Sites', adminOnly: true },
  { to: '/gateways', icon: Network, label: 'Gateways' },
  { to: '/agents', icon: Server, label: 'Agents' },
  { to: '/reports', icon: BarChart3, label: 'Reports' },
  { to: '/enrollment', icon: KeyRound, label: 'Enrollment', adminOnly: true },
  { to: '/import', icon: Upload, label: 'Import' },
  { to: '/settings', icon: Settings, label: 'Settings' },
];

export function Sidebar() {
  const collapsed = useUiStore((s) => s.sidebarCollapsed);
  const toggle = useUiStore((s) => s.toggleSidebar);
  const user = useAuthStore((s) => s.user);
  const isAdmin = user?.role === 'admin';

  const visibleItems = navItems.filter((item) => !item.adminOnly || isAdmin);

  return (
    <aside
      className={cn(
        'fixed left-0 top-0 z-40 h-screen bg-card border-r border-border flex flex-col transition-all duration-200',
        collapsed ? 'w-[60px]' : 'w-[240px]',
      )}
    >
      <div className="flex items-center h-14 px-3 border-b border-border">
        <Shield className="h-6 w-6 text-primary shrink-0" />
        {!collapsed && (
          <span className="ml-2 font-bold text-lg whitespace-nowrap">AppControl</span>
        )}
      </div>

      <nav className="flex-1 py-4 space-y-1 overflow-y-auto px-2">
        {visibleItems.map(({ to, icon: Icon, label }) => (
          <NavLink
            key={to}
            to={to}
            end={to === '/'}
            className={({ isActive }) =>
              cn(
                'flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors',
                isActive
                  ? 'bg-primary text-primary-foreground'
                  : 'text-muted-foreground hover:bg-accent hover:text-accent-foreground',
              )
            }
          >
            <Icon className="h-4 w-4 shrink-0" />
            {!collapsed && <span className="whitespace-nowrap">{label}</span>}
          </NavLink>
        ))}
      </nav>

      <button
        onClick={toggle}
        className="flex items-center justify-center h-10 border-t border-border text-muted-foreground hover:text-foreground transition-colors"
      >
        {collapsed ? <ChevronRight className="h-4 w-4" /> : <ChevronLeft className="h-4 w-4" />}
      </button>
    </aside>
  );
}
