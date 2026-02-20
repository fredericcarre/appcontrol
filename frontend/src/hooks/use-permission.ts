import { useEffectivePermission } from '@/api/permissions';
import { hasPermission, PermissionLevel } from '@/lib/permissions';

export function usePermission(appId: string) {
  const { data: level, isLoading } = useEffectivePermission(appId);

  return {
    level: (level || 'none') as PermissionLevel,
    isLoading,
    can: (required: PermissionLevel) => hasPermission((level || 'none') as PermissionLevel, required),
    canView: hasPermission((level || 'none') as PermissionLevel, 'view'),
    canOperate: hasPermission((level || 'none') as PermissionLevel, 'operate'),
    canEdit: hasPermission((level || 'none') as PermissionLevel, 'edit'),
    canManage: hasPermission((level || 'none') as PermissionLevel, 'manage'),
    isOwner: hasPermission((level || 'none') as PermissionLevel, 'owner'),
  };
}
