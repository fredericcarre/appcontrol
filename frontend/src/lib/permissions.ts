export const PERMISSION_LEVELS = ['none', 'view', 'operate', 'edit', 'manage', 'owner'] as const;
export type PermissionLevel = typeof PERMISSION_LEVELS[number];

export function permissionIndex(level: PermissionLevel): number {
  return PERMISSION_LEVELS.indexOf(level);
}

export function hasPermission(userLevel: PermissionLevel, required: PermissionLevel): boolean {
  return permissionIndex(userLevel) >= permissionIndex(required);
}

export function permissionLabel(level: PermissionLevel): string {
  return level.charAt(0).toUpperCase() + level.slice(1);
}
