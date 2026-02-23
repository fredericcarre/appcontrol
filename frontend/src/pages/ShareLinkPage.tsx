import { useParams, useNavigate } from 'react-router-dom';
import { useConsumeShareLink, useShareLinkInfo } from '@/api/permissions';
import { useAuthStore } from '@/stores/auth';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Shield, CheckCircle, XCircle, Loader2 } from 'lucide-react';
import { useState } from 'react';

export function ShareLinkPage() {
  const { token } = useParams<{ token: string }>();
  const navigate = useNavigate();
  const user = useAuthStore((s) => s.user);
  const { data: info, isLoading, isError } = useShareLinkInfo(token || '');
  const consumeLink = useConsumeShareLink();
  const [accepted, setAccepted] = useState(false);

  if (!token) {
    return <InvalidLink message="No share token provided." />;
  }

  if (isLoading) {
    return (
      <CenterLayout>
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
        <p className="text-muted-foreground mt-2">Loading share link...</p>
      </CenterLayout>
    );
  }

  if (isError || !info) {
    return <InvalidLink message="This share link is invalid or has been revoked." />;
  }

  if (!info.valid) {
    return (
      <InvalidLink
        message={
          info.expired
            ? 'This share link has expired.'
            : 'This share link has reached its maximum number of uses.'
        }
      />
    );
  }

  if (!user) {
    return (
      <CenterLayout>
        <Card className="w-full max-w-md">
          <CardHeader className="text-center">
            <Shield className="h-10 w-10 text-primary mx-auto mb-2" />
            <CardTitle>You've been invited</CardTitle>
          </CardHeader>
          <CardContent className="text-center space-y-4">
            <p className="text-muted-foreground">
              You've been invited to access <strong>{info.app_name}</strong> with{' '}
              <Badge variant="outline">{info.permission_level}</Badge> permission.
            </p>
            <p className="text-sm text-muted-foreground">Log in to accept the invitation.</p>
            <Button className="w-full" onClick={() => navigate(`/login?redirect=/share/${token}`)}>
              Log in to continue
            </Button>
          </CardContent>
        </Card>
      </CenterLayout>
    );
  }

  if (accepted) {
    return (
      <CenterLayout>
        <Card className="w-full max-w-md">
          <CardHeader className="text-center">
            <CheckCircle className="h-10 w-10 text-green-500 mx-auto mb-2" />
            <CardTitle>Access Granted</CardTitle>
          </CardHeader>
          <CardContent className="text-center space-y-4">
            <p className="text-muted-foreground">
              You now have <Badge variant="outline">{info.permission_level}</Badge> access to{' '}
              <strong>{info.app_name}</strong>.
            </p>
            <Button className="w-full" onClick={() => navigate(`/apps/${info.app_id}`)}>
              Open Application
            </Button>
          </CardContent>
        </Card>
      </CenterLayout>
    );
  }

  const handleAccept = async () => {
    await consumeLink.mutateAsync(token);
    setAccepted(true);
  };

  return (
    <CenterLayout>
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <Shield className="h-10 w-10 text-primary mx-auto mb-2" />
          <CardTitle>Accept Invitation</CardTitle>
        </CardHeader>
        <CardContent className="text-center space-y-4">
          <p className="text-muted-foreground">
            You've been invited to access <strong>{info.app_name}</strong> with{' '}
            <Badge variant="outline">{info.permission_level}</Badge> permission.
          </p>
          <div className="flex gap-2">
            <Button variant="outline" className="flex-1" onClick={() => navigate('/')}>
              Decline
            </Button>
            <Button
              className="flex-1"
              onClick={handleAccept}
              disabled={consumeLink.isPending}
            >
              {consumeLink.isPending ? <Loader2 className="h-4 w-4 animate-spin mr-1" /> : null}
              Accept
            </Button>
          </div>
          {consumeLink.isError && (
            <p className="text-sm text-destructive">Failed to accept invitation. Please try again.</p>
          )}
        </CardContent>
      </Card>
    </CenterLayout>
  );
}

function CenterLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-center min-h-screen bg-background p-4">
      <div className="w-full max-w-md">{children}</div>
    </div>
  );
}

function InvalidLink({ message }: { message: string }) {
  return (
    <CenterLayout>
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <XCircle className="h-10 w-10 text-destructive mx-auto mb-2" />
          <CardTitle>Invalid Link</CardTitle>
        </CardHeader>
        <CardContent className="text-center">
          <p className="text-muted-foreground">{message}</p>
        </CardContent>
      </Card>
    </CenterLayout>
  );
}
