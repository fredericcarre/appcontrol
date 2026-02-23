import { describe, it, expect } from 'vitest';
import {
  STATE_COLORS,
  COMPONENT_TYPE_ICONS,
  ERROR_BRANCH_COLORS,
  ComponentState,
  ComponentType,
} from './colors';

describe('STATE_COLORS', () => {
  it('should define colors for all FSM states', () => {
    const expectedStates: ComponentState[] = [
      'RUNNING', 'DEGRADED', 'FAILED', 'STOPPED',
      'STARTING', 'STOPPING', 'UNREACHABLE', 'UNKNOWN',
    ];

    expectedStates.forEach((state) => {
      expect(STATE_COLORS[state]).toBeDefined();
      expect(STATE_COLORS[state].bg).toBeDefined();
      expect(STATE_COLORS[state].border).toBeDefined();
      expect(STATE_COLORS[state].animation).toBeDefined();
    });
  });

  it('should have correct RUNNING colors', () => {
    expect(STATE_COLORS.RUNNING.bg).toBe('#E8F5E9');
    expect(STATE_COLORS.RUNNING.border).toBe('#4CAF50');
    expect(STATE_COLORS.RUNNING.animation).toBe('none');
  });

  it('should have correct FAILED colors', () => {
    expect(STATE_COLORS.FAILED.bg).toBe('#FFEBEE');
    expect(STATE_COLORS.FAILED.border).toBe('#F44336');
    expect(STATE_COLORS.FAILED.animation).toBe('none');
  });

  it('should have correct DEGRADED colors', () => {
    expect(STATE_COLORS.DEGRADED.bg).toBe('#FFF3E0');
    expect(STATE_COLORS.DEGRADED.border).toBe('#FF9800');
    expect(STATE_COLORS.DEGRADED.animation).toBe('none');
  });

  it('should have correct STOPPED colors', () => {
    expect(STATE_COLORS.STOPPED.bg).toBe('#F5F5F5');
    expect(STATE_COLORS.STOPPED.border).toBe('#9E9E9E');
    expect(STATE_COLORS.STOPPED.animation).toBe('none');
  });

  it('should have pulse animation for STARTING', () => {
    expect(STATE_COLORS.STARTING.animation).toBe('pulse 1.5s ease-in-out infinite');
    expect(STATE_COLORS.STARTING.bg).toBe('#E3F2FD');
    expect(STATE_COLORS.STARTING.border).toBe('#2196F3');
  });

  it('should have pulse animation for STOPPING', () => {
    expect(STATE_COLORS.STOPPING.animation).toBe('pulse 1.5s ease-in-out infinite');
    expect(STATE_COLORS.STOPPING.bg).toBe('#E3F2FD');
    expect(STATE_COLORS.STOPPING.border).toBe('#2196F3');
  });

  it('should have correct UNREACHABLE colors', () => {
    expect(STATE_COLORS.UNREACHABLE.bg).toBe('rgba(33,33,33,0.1)');
    expect(STATE_COLORS.UNREACHABLE.border).toBe('#212121');
    expect(STATE_COLORS.UNREACHABLE.animation).toBe('none');
  });

  it('should have dashed border style for UNKNOWN', () => {
    expect(STATE_COLORS.UNKNOWN.bg).toBe('#FFFFFF');
    expect(STATE_COLORS.UNKNOWN.border).toBe('#BDBDBD');
    expect(STATE_COLORS.UNKNOWN.borderStyle).toBe('dashed');
  });

  it('should have no animation for static states', () => {
    const staticStates: ComponentState[] = ['RUNNING', 'DEGRADED', 'FAILED', 'STOPPED', 'UNREACHABLE', 'UNKNOWN'];
    staticStates.forEach((state) => {
      expect(STATE_COLORS[state].animation).toBe('none');
    });
  });
});

describe('COMPONENT_TYPE_ICONS', () => {
  it('should define icons for all component types', () => {
    const expectedTypes: ComponentType[] = [
      'database', 'middleware', 'appserver', 'webfront',
      'service', 'batch', 'custom',
    ];

    expectedTypes.forEach((type) => {
      expect(COMPONENT_TYPE_ICONS[type]).toBeDefined();
      expect(COMPONENT_TYPE_ICONS[type].icon).toBeDefined();
      expect(COMPONENT_TYPE_ICONS[type].color).toBeDefined();
    });
  });

  it('should have correct database icon config', () => {
    expect(COMPONENT_TYPE_ICONS.database.icon).toBe('Database');
    expect(COMPONENT_TYPE_ICONS.database.color).toBe('#1565C0');
  });

  it('should have correct middleware icon config', () => {
    expect(COMPONENT_TYPE_ICONS.middleware.icon).toBe('Layers');
    expect(COMPONENT_TYPE_ICONS.middleware.color).toBe('#6A1B9A');
  });

  it('should have correct appserver icon config', () => {
    expect(COMPONENT_TYPE_ICONS.appserver.icon).toBe('Server');
    expect(COMPONENT_TYPE_ICONS.appserver.color).toBe('#2E7D32');
  });

  it('should have correct webfront icon config', () => {
    expect(COMPONENT_TYPE_ICONS.webfront.icon).toBe('Globe');
    expect(COMPONENT_TYPE_ICONS.webfront.color).toBe('#E65100');
  });

  it('should have correct service icon config', () => {
    expect(COMPONENT_TYPE_ICONS.service.icon).toBe('Cog');
    expect(COMPONENT_TYPE_ICONS.service.color).toBe('#37474F');
  });

  it('should have correct batch icon config', () => {
    expect(COMPONENT_TYPE_ICONS.batch.icon).toBe('Clock');
    expect(COMPONENT_TYPE_ICONS.batch.color).toBe('#4E342E');
  });

  it('should have correct custom icon config', () => {
    expect(COMPONENT_TYPE_ICONS.custom.icon).toBe('Box');
    expect(COMPONENT_TYPE_ICONS.custom.color).toBe('#455A64');
  });
});

describe('ERROR_BRANCH_COLORS', () => {
  it('should have correct error branch background', () => {
    expect(ERROR_BRANCH_COLORS.bg).toBe('#FFE0E6');
  });

  it('should have correct error branch border', () => {
    expect(ERROR_BRANCH_COLORS.border).toBe('#FF6B8A');
  });
});
