import { describe, it, expect } from 'vitest';
import { permissionIndex, hasPermission, permissionLabel, PERMISSION_LEVELS } from './permissions';

describe('permissions', () => {
  it('should have correct permission ordering', () => {
    expect(permissionIndex('none')).toBe(0);
    expect(permissionIndex('view')).toBe(1);
    expect(permissionIndex('operate')).toBe(2);
    expect(permissionIndex('edit')).toBe(3);
    expect(permissionIndex('manage')).toBe(4);
    expect(permissionIndex('owner')).toBe(5);
  });

  it('should check permissions correctly', () => {
    expect(hasPermission('owner', 'view')).toBe(true);
    expect(hasPermission('view', 'owner')).toBe(false);
    expect(hasPermission('operate', 'operate')).toBe(true);
    expect(hasPermission('none', 'view')).toBe(false);
    expect(hasPermission('edit', 'operate')).toBe(true);
    expect(hasPermission('manage', 'edit')).toBe(true);
  });

  it('should generate correct labels', () => {
    expect(permissionLabel('none')).toBe('None');
    expect(permissionLabel('view')).toBe('View');
    expect(permissionLabel('owner')).toBe('Owner');
  });

  it('should have 6 permission levels', () => {
    expect(PERMISSION_LEVELS).toHaveLength(6);
  });
});
