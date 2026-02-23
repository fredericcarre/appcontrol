import { useNavigate } from 'react-router-dom';
import { useAuthStore } from '@/stores/auth';
import { useUiStore } from '@/stores/ui';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Moon, Sun, User, Key } from 'lucide-react';

export function SettingsPage() {
  const navigate = useNavigate();
  const user = useAuthStore((s) => s.user);
  const theme = useUiStore((s) => s.theme);
  const toggleTheme = useUiStore((s) => s.toggleTheme);

  return (
    <div className="space-y-6 max-w-2xl">
      <h1 className="text-2xl font-bold">Settings</h1>

      <Card>
        <CardHeader>
          <CardTitle className="text-lg flex items-center gap-2">
            <User className="h-5 w-5" /> Profile
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <span className="text-muted-foreground">Name</span>
              <p className="font-medium">{user?.name || '-'}</p>
            </div>
            <div>
              <span className="text-muted-foreground">Email</span>
              <p className="font-medium">{user?.email || '-'}</p>
            </div>
            <div>
              <span className="text-muted-foreground">Role</span>
              <p className="font-medium capitalize">{user?.role || '-'}</p>
            </div>
            <div>
              <span className="text-muted-foreground">Organization</span>
              <p className="font-medium font-mono text-xs">{user?.org_id?.slice(0, 8) || '-'}</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-lg flex items-center gap-2">
            <Key className="h-5 w-5" /> API Keys
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm font-medium">Manage API Keys</p>
              <p className="text-xs text-muted-foreground">Create and revoke API keys for scheduler integration</p>
            </div>
            <Button variant="outline" size="sm" onClick={() => navigate('/settings/api-keys')}>
              Manage Keys
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-lg">Appearance</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm font-medium">Theme</p>
              <p className="text-xs text-muted-foreground">Toggle between light and dark mode</p>
            </div>
            <Button variant="outline" size="sm" onClick={toggleTheme}>
              {theme === 'light' ? (
                <><Moon className="h-4 w-4 mr-2" /> Dark Mode</>
              ) : (
                <><Sun className="h-4 w-4 mr-2" /> Light Mode</>
              )}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
