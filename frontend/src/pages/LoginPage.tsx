import { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuthStore } from '@/stores/auth';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from '@/components/ui/card';
import { Shield } from 'lucide-react';
import client from '@/api/client';

interface AuthInfo {
  local: boolean;
  oidc: boolean;
  saml: boolean;
}

export function LoginPage() {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [authInfo, setAuthInfo] = useState<AuthInfo | null>(null);
  const setAuth = useAuthStore((s) => s.setAuth);
  const navigate = useNavigate();

  // Fetch auth info to know which login methods are available
  useEffect(() => {
    client.get('/auth/info').then(({ data }) => {
      setAuthInfo(data);
    }).catch(() => {
      // Backend not reachable — assume local only
      setAuthInfo({ local: true, oidc: false, saml: false });
    });
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');
    setLoading(true);

    try {
      const { data } = await client.post('/auth/login', {
        email,
        password,
      });
      setAuth(data.token, data.user);
      navigate('/');
    } catch (err: unknown) {
      const axiosErr = err as { response?: { data?: { message?: string } } };
      setError(axiosErr.response?.data?.message || 'Login failed');
    } finally {
      setLoading(false);
    }
  };

  const hasSso = authInfo?.oidc || authInfo?.saml;

  return (
    <div className="min-h-screen flex items-center justify-center bg-background p-4">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <div className="flex justify-center mb-4">
            <div className="p-3 rounded-full bg-primary/10">
              <Shield className="h-8 w-8 text-primary" />
            </div>
          </div>
          <CardTitle className="text-2xl">AppControl</CardTitle>
          <CardDescription>Sign in to your account</CardDescription>
        </CardHeader>
        <form onSubmit={handleSubmit}>
          <CardContent className="space-y-4">
            {error && (
              <div className="p-3 rounded-md bg-destructive/10 text-destructive text-sm">
                {error}
              </div>
            )}
            <div className="space-y-2">
              <label className="text-sm font-medium" htmlFor="email">Email</label>
              <Input
                id="email"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                required
              />
            </div>
            <div className="space-y-2">
              <label className="text-sm font-medium" htmlFor="password">Password</label>
              <Input
                id="password"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
              />
            </div>
          </CardContent>
          <CardFooter className="flex-col gap-3">
            <Button type="submit" className="w-full" disabled={loading}>
              {loading ? 'Signing in...' : 'Sign in'}
            </Button>
            {hasSso && (
              <Button type="button" variant="outline" className="w-full" onClick={() => {
                // Prefer OIDC if available, otherwise SAML
                const ssoPath = authInfo?.oidc ? '/api/v1/auth/oidc/login' : '/api/v1/auth/saml/login';
                window.location.href = ssoPath;
              }}>
                Sign in with SSO
              </Button>
            )}
          </CardFooter>
        </form>
      </Card>
    </div>
  );
}
