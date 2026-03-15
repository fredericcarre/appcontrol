import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { DetailPanel } from './DetailPanel';
import type { Component } from '@/api/apps';

// Mock the API hooks to avoid needing a real backend
vi.mock('@/api/components', () => ({
  useStateTransitions: () => ({ data: undefined }),
  useCommandExecutions: () => ({ data: undefined }),
  useCheckEvents: () => ({ data: undefined }),
}));

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
    current_state: 'RUNNING',
    check_cmd: '/usr/local/bin/check_db.sh',
    start_cmd: '/usr/local/bin/start_db.sh',
    stop_cmd: '/usr/local/bin/stop_db.sh',
    restart_cmd: null,
    check_interval_seconds: 30,
    is_optional: false,
    agent_id: 'agent-1',
    group_name: null,
    display_order: 1,
    position_x: null,
    position_y: null,
    created_at: '2024-01-01T00:00:00Z',
    updated_at: '2024-01-01T00:00:00Z',
    ...overrides,
  };
}

function renderWithProviders(ui: React.ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>,
  );
}

describe('DetailPanel', () => {
  it('should render component name', () => {
    renderWithProviders(<DetailPanel component={createComponent()} onClose={vi.fn()} />);
    expect(screen.getByText('my-database')).toBeInTheDocument();
  });

  it('should render component host', () => {
    renderWithProviders(<DetailPanel component={createComponent()} onClose={vi.fn()} />);
    const hostElements = screen.getAllByText('db-server-01');
    expect(hostElements.length).toBeGreaterThanOrEqual(1);
  });

  it('should render component state', () => {
    renderWithProviders(<DetailPanel component={createComponent({ state: 'RUNNING' })} onClose={vi.fn()} />);
    expect(screen.getByText('RUNNING')).toBeInTheDocument();
  });

  it('should render component type badge', () => {
    renderWithProviders(<DetailPanel component={createComponent({ component_type: 'database' })} onClose={vi.fn()} />);
    expect(screen.getByText('database')).toBeInTheDocument();
  });

  it('should call onClose when close button is clicked', () => {
    const onClose = vi.fn();
    renderWithProviders(<DetailPanel component={createComponent()} onClose={onClose} />);

    const buttons = screen.getAllByRole('button');
    const closeButton = buttons[0];
    fireEvent.click(closeButton);

    expect(onClose).toHaveBeenCalled();
  });

  it('should show action buttons when canOperate is true', () => {
    renderWithProviders(
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
    renderWithProviders(
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
    renderWithProviders(
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
    renderWithProviders(
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

  it('should show info tab content when Info tab is clicked', () => {
    renderWithProviders(
      <DetailPanel
        component={createComponent({ check_interval_seconds: 30, is_optional: true })}
        onClose={vi.fn()}
      />,
    );

    // Click on Info tab (default is now Metrics)
    fireEvent.click(screen.getByText('Info'));

    expect(screen.getByText('30s')).toBeInTheDocument();
    expect(screen.getByText('Yes')).toBeInTheDocument();
  });

  it('should show "No" for non-optional components in info tab', () => {
    renderWithProviders(
      <DetailPanel
        component={createComponent({ is_optional: false })}
        onClose={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText('Info'));

    expect(screen.getByText('No')).toBeInTheDocument();
  });

  it('should display check command in info tab', () => {
    renderWithProviders(
      <DetailPanel
        component={createComponent({ check_cmd: '/bin/health_check' })}
        onClose={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText('Info'));

    expect(screen.getByText('/bin/health_check')).toBeInTheDocument();
  });

  it('should display start command in info tab', () => {
    renderWithProviders(
      <DetailPanel
        component={createComponent({ start_cmd: '/bin/start_service' })}
        onClose={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText('Info'));

    expect(screen.getByText('/bin/start_service')).toBeInTheDocument();
  });

  it('should display stop command in info tab', () => {
    renderWithProviders(
      <DetailPanel
        component={createComponent({ stop_cmd: '/bin/stop_service' })}
        onClose={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText('Info'));

    expect(screen.getByText('/bin/stop_service')).toBeInTheDocument();
  });

  it('should render tabs for Info, Commands, and Events', () => {
    renderWithProviders(
      <DetailPanel component={createComponent()} onClose={vi.fn()} />
    );

    expect(screen.getByText('Info')).toBeInTheDocument();
    expect(screen.getByText('Commands')).toBeInTheDocument();
    expect(screen.getByText('Events')).toBeInTheDocument();
  });

  it('should show command buttons in Commands tab when canOperate', () => {
    renderWithProviders(
      <DetailPanel
        component={createComponent()}
        onClose={vi.fn()}
        onCommand={vi.fn()}
        onDiagnose={vi.fn()}
        canOperate={true}
      />,
    );

    fireEvent.click(screen.getByText('Commands'));

    expect(screen.getByText('Execute Custom Command')).toBeInTheDocument();
    expect(screen.getByText('Run Diagnostic')).toBeInTheDocument();
  });

  it('should display UNKNOWN state for missing state', () => {
    renderWithProviders(
      <DetailPanel
        component={createComponent({ current_state: '' })}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('UNKNOWN')).toBeInTheDocument();
  });
});
