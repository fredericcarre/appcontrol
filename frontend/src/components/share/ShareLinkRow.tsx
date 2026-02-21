import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Copy, Clock } from 'lucide-react';
import { ShareLink } from '@/api/permissions';

interface ShareLinkRowProps {
  link: ShareLink;
}

export function ShareLinkRow({ link }: ShareLinkRowProps) {
  const copyToClipboard = () => {
    navigator.clipboard.writeText(`${window.location.origin}/share/${link.token}`);
  };

  return (
    <div className="flex items-center justify-between p-2 rounded-md hover:bg-muted">
      <div className="flex items-center gap-2">
        <Badge variant="outline">{link.permission_level}</Badge>
        <span className="text-xs text-muted-foreground">
          {link.current_uses}{link.max_uses ? `/${link.max_uses}` : ''} uses
        </span>
        {link.expires_at && (
          <span className="text-xs text-muted-foreground flex items-center gap-1">
            <Clock className="h-3 w-3" />
            {new Date(link.expires_at).toLocaleDateString()}
          </span>
        )}
      </div>
      <Button variant="ghost" size="icon" className="h-7 w-7" onClick={copyToClipboard}>
        <Copy className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}
