import { useState } from 'react';
import { useAuthStore } from '@/stores/auth';
import { useUiStore } from '@/stores/ui';
import { useWebSocketStore } from '@/stores/websocket';
import { Button } from '@/components/ui/button';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Breadcrumbs } from '@/components/layout/Breadcrumb';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { Moon, Sun, Wifi, WifiOff, LogOut } from 'lucide-react';

export function Header() {
  const user = useAuthStore((s) => s.user);
  const logout = useAuthStore((s) => s.logout);
  const theme = useUiStore((s) => s.theme);
  const toggleTheme = useUiStore((s) => s.toggleTheme);
  const wsConnected = useWebSocketStore((s) => s.connected);
  const [logoutConfirm, setLogoutConfirm] = useState(false);

  // Use name if available, otherwise extract from email
  const displayName = user?.name || user?.email?.split('@')[0] || '';
  const initials = displayName
    ? displayName.split(/[\s._-]/).map((n) => n[0]).join('').toUpperCase().slice(0, 2)
    : '?';

  const handleLogout = () => {
    setLogoutConfirm(false);
    logout();
  };

  return (
    <>
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
              <span className="text-sm font-medium hidden md:inline">{displayName}</span>
            )}
          </div>

          <Button variant="ghost" size="icon" onClick={() => setLogoutConfirm(true)}>
            <LogOut className="h-4 w-4" />
          </Button>
        </div>
      </header>

      <Dialog open={logoutConfirm} onOpenChange={setLogoutConfirm}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Sign out</DialogTitle>
            <DialogDescription>
              Are you sure you want to sign out of your account?
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setLogoutConfirm(false)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleLogout}>
              Sign out
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
