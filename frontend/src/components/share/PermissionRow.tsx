import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Trash2 } from 'lucide-react';
import { AppPermission } from '@/api/permissions';

interface PermissionRowProps {
  permission: AppPermission;
  onRemove: () => void;
}

export function PermissionRow({ permission, onRemove }: PermissionRowProps) {
  const label = permission.user_email || permission.team_name || 'Unknown';
  const initials = label.slice(0, 2).toUpperCase();

  return (
    <div className="flex items-center gap-3 p-2 rounded-md hover:bg-muted">
      <Avatar className="h-8 w-8">
        <AvatarFallback className="text-xs">{initials}</AvatarFallback>
      </Avatar>
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium truncate">{label}</p>
        {permission.team_id && (
          <p className="text-xs text-muted-foreground">Team</p>
        )}
      </div>
      <Badge variant="outline">{permission.level}</Badge>
      <Button variant="ghost" size="icon" className="h-7 w-7" onClick={onRemove}>
        <Trash2 className="h-3.5 w-3.5 text-destructive" />
      </Button>
    </div>
  );
}
