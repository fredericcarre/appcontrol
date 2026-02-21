import { useAuthStore } from '@/stores/auth';
import { useUiStore } from '@/stores/ui';
import { useWebSocketStore } from '@/stores/websocket';
import { Button } from '@/components/ui/button';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Breadcrumbs } from '@/components/layout/Breadcrumb';
import { Moon, Sun, Wifi, WifiOff, LogOut } from 'lucide-react';

export function Header() {
  const user = useAuthStore((s) => s.user);
  const logout = useAuthStore((s) => s.logout);
  const theme = useUiStore((s) => s.theme);
  const toggleTheme = useUiStore((s) => s.toggleTheme);
  const wsConnected = useWebSocketStore((s) => s.connected);

  const initials = user?.name
    ? user.name.split(' ').map((n) => n[0]).join('').toUpperCase().slice(0, 2)
    : '??';

  return (
    <header className="h-14 border-b border-border flex items-center justify-between px-6 bg-card">
      <Breadcrumbs />

      <div className="flex items-center gap-3">
        <div className="flex items-center gap-1 text-xs text-muted-foreground">
          {wsConnected ? (
            <Wifi className="h-3.5 w-3.5 text-state-running" />
          ) : (
            <WifiOff className="h-3.5 w-3.5 text-state-failed" />
          )}
          <span>{wsConnected ? 'Connected' : 'Offline'}</span>
        </div>

        <Button variant="ghost" size="icon" onClick={toggleTheme}>
          {theme === 'light' ? <Moon className="h-4 w-4" /> : <Sun className="h-4 w-4" />}
        </Button>

        <div className="flex items-center gap-2">
          <Avatar className="h-8 w-8">
            <AvatarFallback className="text-xs">{initials}</AvatarFallback>
          </Avatar>
          {user && (
            <span className="text-sm font-medium hidden md:inline">{user.name}</span>
          )}
        </div>

        <Button variant="ghost" size="icon" onClick={logout}>
          <LogOut className="h-4 w-4" />
        </Button>
      </div>
    </header>
  );
}
