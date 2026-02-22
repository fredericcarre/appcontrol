import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { DetailPanel } from './DetailPanel';
import type { Component } from '@/api/apps';

function createComponent(overrides: Partial<Component> = {}): Component {
  return {
    id: 'comp-1',
    app_id: 'app-1',
    name: 'my-database',
    display_name: null,
    description: null,
    icon: null,
    group_id: null,
    host: 'db-server-01',
    component_type: 'database',
    state: 'RUNNING',
    check_cmd: '/usr/local/bin/check_db.sh',
    start_cmd: '/usr/local/bin/start_db.sh',
    stop_cmd: '/usr/local/bin/stop_db.sh',
    restart_cmd: null,
    check_interval_secs: 30,
    agent_id: 'agent-1',
    group_name: null,
    display_order: 1,
    position_x: null,
    position_y: null,
    is_protected: false,
    created_at: '2024-01-01T00:00:00Z',
    updated_at: '2024-01-01T00:00:00Z',
    ...overrides,
  };
}

describe('DetailPanel', () => {
  it('should render component name', () => {
    render(<DetailPanel component={createComponent()} onClose={vi.fn()} />);
    expect(screen.getByText('my-database')).toBeInTheDocument();
  });

  it('should render component host', () => {
    render(<DetailPanel component={createComponent()} onClose={vi.fn()} />);
    expect(screen.getByText('db-server-01')).toBeInTheDocument();
  });

  it('should render component state', () => {
    render(<DetailPanel component={createComponent({ state: 'RUNNING' })} onClose={vi.fn()} />);
    expect(screen.getByText('RUNNING')).toBeInTheDocument();
  });

  it('should render component type badge', () => {
    render(<DetailPanel component={createComponent({ component_type: 'database' })} onClose={vi.fn()} />);
    expect(screen.getByText('database')).toBeInTheDocument();
  });

  it('should call onClose when close button is clicked', () => {
    const onClose = vi.fn();
    render(<DetailPanel component={createComponent()} onClose={onClose} />);

    // Find the close button (has X icon)
    const buttons = screen.getAllByRole('button');
    const closeButton = buttons[0]; // First button is the close button
    fireEvent.click(closeButton);

    expect(onClose).toHaveBeenCalled();
  });

  it('should show action buttons when canOperate is true', () => {
    render(
      <DetailPanel
        component={createComponent()}
        onClose={vi.fn()}
        onStart={vi.fn()}
        onStop={vi.fn()}
        onRestart={vi.fn()}
        canOperate={true}
      />,
    );

    expect(screen.getByText('Start')).toBeInTheDocument();
    expect(screen.getByText('Stop')).toBeInTheDocument();
  });

  it('should hide action buttons when canOperate is false', () => {
    render(
      <DetailPanel
        component={createComponent()}
        onClose={vi.fn()}
        canOperate={false}
      />,
    );

    expect(screen.queryByText('Start')).not.toBeInTheDocument();
    expect(screen.queryByText('Stop')).not.toBeInTheDocument();
  });

  it('should call onStart when Start button is clicked', () => {
    const onStart = vi.fn();
    render(
      <DetailPanel
        component={createComponent()}
        onClose={vi.fn()}
        onStart={onStart}
        onStop={vi.fn()}
        onRestart={vi.fn()}
        canOperate={true}
      />,
    );

    fireEvent.click(screen.getByText('Start'));
    expect(onStart).toHaveBeenCalled();
  });

  it('should call onStop when Stop button is clicked', () => {
    const onStop = vi.fn();
    render(
      <DetailPanel
        component={createComponent()}
        onClose={vi.fn()}
        onStart={vi.fn()}
        onStop={onStop}
        onRestart={vi.fn()}
        canOperate={true}
      />,
    );

    fireEvent.click(screen.getByText('Stop'));
    expect(onStop).toHaveBeenCalled();
  });

  it('should show info tab by default with component details', () => {
    render(
      <DetailPanel
        component={createComponent({ check_interval_secs: 30, is_protected: true })}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('30s')).toBeInTheDocument();
    expect(screen.getByText('Yes')).toBeInTheDocument(); // Protected: Yes
  });

  it('should show "No" for non-protected components', () => {
    render(
      <DetailPanel
        component={createComponent({ is_protected: false })}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('No')).toBeInTheDocument();
  });

  it('should display check command in info tab', () => {
    render(
      <DetailPanel
        component={createComponent({ check_cmd: '/bin/health_check' })}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('/bin/health_check')).toBeInTheDocument();
  });

  it('should display start command in info tab', () => {
    render(
      <DetailPanel
        component={createComponent({ start_cmd: '/bin/start_service' })}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('/bin/start_service')).toBeInTheDocument();
  });

  it('should display stop command in info tab', () => {
    render(
      <DetailPanel
        component={createComponent({ stop_cmd: '/bin/stop_service' })}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('/bin/stop_service')).toBeInTheDocument();
  });

  it('should render tabs for Info, Commands, and Events', () => {
    render(
      <DetailPanel component={createComponent()} onClose={vi.fn()} />
    );

    expect(screen.getByRole('tab', { name: 'Info' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Commands' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Events' })).toBeInTheDocument();
  });

  it('should show command buttons in Commands tab when canOperate', () => {
    render(
      <DetailPanel
        component={createComponent()}
        onClose={vi.fn()}
        onCommand={vi.fn()}
        onDiagnose={vi.fn()}
        canOperate={true}
      />,
    );

    // Switch to Commands tab
    fireEvent.click(screen.getByRole('tab', { name: 'Commands' }));

    expect(screen.getByText('Execute Custom Command')).toBeInTheDocument();
    expect(screen.getByText('Run Diagnostic')).toBeInTheDocument();
  });

  it('should display UNKNOWN state for missing state', () => {
    render(
      <DetailPanel
        component={createComponent({ state: '' })}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('UNKNOWN')).toBeInTheDocument();
  });
});
