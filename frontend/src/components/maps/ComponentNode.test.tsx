import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ReactFlowProvider } from '@xyflow/react';
import React from 'react';

// We need to test ComponentNode, which uses React Flow handles
// Mock @xyflow/react to avoid requiring a full ReactFlow context for simple rendering
vi.mock('@xyflow/react', () => ({
  Handle: ({ type, position }: { type: string; position: string }) =>
    React.createElement('div', { 'data-testid': `handle-${type}`, 'data-position': position }),
  Position: { Top: 'top', Bottom: 'bottom' },
  ReactFlowProvider: ({ children }: { children: React.ReactNode }) =>
    React.createElement('div', null, children),
}));

import { ComponentNode } from './ComponentNode';

function renderComponentNode(overrides: Record<string, unknown> = {}, selected = false) {
  const defaultData = {
    label: 'my-db',
    state: 'RUNNING' as const,
    componentType: 'database' as const,
    host: 'db-server-01',
    ...overrides,
  };

  return render(
    <ReactFlowProvider>
      <ComponentNode
        id="node-1"
        data={defaultData as never}
        selected={selected}
        type="custom"
        isConnectable={true}
        positionAbsoluteX={0}
        positionAbsoluteY={0}
        zIndex={0}
        dragging={false}
        dragHandle=""
        parentId=""
        sourcePosition={undefined}
        targetPosition={undefined}
        width={180}
        height={80}
        deletable={false}
        selectable={true}
        connectable={true}
        focusable={true}
        measured={{ width: 180, height: 80 }}
      />
    </ReactFlowProvider>,
  );
}

describe('ComponentNode', () => {
  it('should render the component label', () => {
    renderComponentNode();
    expect(screen.getByText('my-db')).toBeInTheDocument();
  });

  it('should render the displayName when provided', () => {
    renderComponentNode({ displayName: 'My Database' });
    expect(screen.getByText('My Database')).toBeInTheDocument();
  });

  it('should fallback to label when no displayName', () => {
    renderComponentNode({ label: 'fallback-label', displayName: undefined });
    expect(screen.getByText('fallback-label')).toBeInTheDocument();
  });

  it('should render the host name', () => {
    renderComponentNode({ host: 'prod-server-01' });
    expect(screen.getByText('prod-server-01')).toBeInTheDocument();
  });

  it('should render the state text', () => {
    renderComponentNode({ state: 'RUNNING' });
    expect(screen.getByText('RUNNING')).toBeInTheDocument();
  });

  it('should render FAILED state', () => {
    renderComponentNode({ state: 'FAILED' });
    expect(screen.getByText('FAILED')).toBeInTheDocument();
  });

  it('should render STOPPED state', () => {
    renderComponentNode({ state: 'STOPPED' });
    expect(screen.getByText('STOPPED')).toBeInTheDocument();
  });

  it('should apply RUNNING background color', () => {
    renderComponentNode({ state: 'RUNNING' });
    // Get the outermost styled container (the node wrapper with border)
    const stateEl = screen.getByText('RUNNING');
    const container = stateEl.closest('.rounded-lg') as HTMLElement;
    expect(container).toBeTruthy();
    // jsdom converts hex to rgb, so check for the rgb equivalent of #E8F5E9
    expect(container?.style.backgroundColor).toBe('rgb(232, 245, 233)');
  });

  it('should apply FAILED background color', () => {
    renderComponentNode({ state: 'FAILED' });
    const container = screen.getByText('FAILED').closest('.rounded-lg') as HTMLElement;
    // rgb of #FFEBEE
    expect(container?.style.backgroundColor).toBe('rgb(255, 235, 238)');
  });

  it('should apply dashed border for UNKNOWN state', () => {
    renderComponentNode({ state: 'UNKNOWN' });
    const container = screen.getByText('UNKNOWN').closest('.rounded-lg') as HTMLElement;
    expect(container?.style.borderStyle).toBe('dashed');
  });

  it('should apply error branch colors when isErrorBranch is true', () => {
    renderComponentNode({ isErrorBranch: true });
    const container = screen.getByText('my-db').closest('.rounded-lg') as HTMLElement;
    // rgb of #FFE0E6
    expect(container?.style.backgroundColor).toBe('rgb(255, 224, 230)');
    // rgb of #FF6B8A
    expect(container?.style.borderColor).toBe('rgb(255, 107, 138)');
  });

  it('should add pulse animation class for STARTING state', () => {
    renderComponentNode({ state: 'STARTING' });
    const container = screen.getByText('STARTING').closest('.animate-state-pulse');
    expect(container).toBeInTheDocument();
  });

  it('should add pulse animation class for STOPPING state', () => {
    renderComponentNode({ state: 'STOPPING' });
    const container = screen.getByText('STOPPING').closest('.animate-state-pulse');
    expect(container).toBeInTheDocument();
  });

  it('should not show action buttons when not selected', () => {
    renderComponentNode({}, false);
    expect(screen.queryByTitle('Start')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Stop')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Restart')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Diagnose')).not.toBeInTheDocument();
  });

  it('should show action buttons when selected', () => {
    renderComponentNode({}, true);
    expect(screen.getByTitle('Start')).toBeInTheDocument();
    expect(screen.getByTitle('Stop')).toBeInTheDocument();
    expect(screen.getByTitle('Restart')).toBeInTheDocument();
    expect(screen.getByTitle('Diagnose')).toBeInTheDocument();
  });

  it('should call onStart when Start button is clicked', () => {
    const onStart = vi.fn();
    renderComponentNode({ onStart }, true);

    fireEvent.click(screen.getByTitle('Start'));
    expect(onStart).toHaveBeenCalledWith('node-1');
  });

  it('should call onStop when Stop button is clicked', () => {
    const onStop = vi.fn();
    renderComponentNode({ onStop }, true);

    fireEvent.click(screen.getByTitle('Stop'));
    expect(onStop).toHaveBeenCalledWith('node-1');
  });

  it('should call onRestart when Restart button is clicked', () => {
    const onRestart = vi.fn();
    renderComponentNode({ onRestart }, true);

    fireEvent.click(screen.getByTitle('Restart'));
    expect(onRestart).toHaveBeenCalledWith('node-1');
  });

  it('should call onDiagnose when Diagnose button is clicked', () => {
    const onDiagnose = vi.fn();
    renderComponentNode({ onDiagnose }, true);

    fireEvent.click(screen.getByTitle('Diagnose'));
    expect(onDiagnose).toHaveBeenCalledWith('node-1');
  });

  it('should render React Flow handles', () => {
    renderComponentNode();
    expect(screen.getByTestId('handle-target')).toBeInTheDocument();
    expect(screen.getByTestId('handle-source')).toBeInTheDocument();
  });

  it('should show links when selected and links provided', () => {
    renderComponentNode({
      links: [
        { label: 'Docs', url: 'https://docs.example.com' },
        { label: 'Logs', url: 'https://logs.example.com' },
      ],
    }, true);

    expect(screen.getByText('Docs')).toBeInTheDocument();
    expect(screen.getByText('Logs')).toBeInTheDocument();

    const docsLink = screen.getByText('Docs').closest('a');
    expect(docsLink).toHaveAttribute('href', 'https://docs.example.com');
    expect(docsLink).toHaveAttribute('target', '_blank');
    expect(docsLink).toHaveAttribute('rel', 'noopener noreferrer');
  });

  it('should not show links when not selected', () => {
    renderComponentNode({
      links: [{ label: 'Docs', url: 'https://docs.example.com' }],
    }, false);

    expect(screen.queryByText('Docs')).not.toBeInTheDocument();
  });

  it('should apply group color to left border', () => {
    renderComponentNode({ groupColor: '#FF0000' });
    const container = screen.getByText('my-db').closest('.rounded-lg') as HTMLElement;
    expect(container?.style.borderLeftColor).toBe('rgb(255, 0, 0)');
  });

  it('should show ring when selected', () => {
    renderComponentNode({}, true);
    const container = screen.getByText('my-db').closest('.ring-2');
    expect(container).toBeInTheDocument();
  });

  it('should set title from description', () => {
    renderComponentNode({ description: 'Main production database' });
    const label = screen.getByText('my-db');
    expect(label.closest('[title]')?.getAttribute('title')).toBe('Main production database');
  });
});
