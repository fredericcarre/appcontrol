import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { useUiStore } from '@/stores/ui';
import { Sidebar } from './Sidebar';

function renderSidebar(route = '/') {
  return render(
    <MemoryRouter initialEntries={[route]}>
      <Sidebar />
    </MemoryRouter>,
  );
}

describe('Sidebar', () => {
  beforeEach(() => {
    useUiStore.setState({ sidebarCollapsed: false });
  });

  it('should render the AppControl brand name when expanded', () => {
    renderSidebar();
    expect(screen.getByText('AppControl')).toBeInTheDocument();
  });

  it('should hide the brand name when collapsed', () => {
    useUiStore.setState({ sidebarCollapsed: true });
    renderSidebar();
    expect(screen.queryByText('AppControl')).not.toBeInTheDocument();
  });

  it('should render all navigation items', () => {
    renderSidebar();
    expect(screen.getByText('Dashboard')).toBeInTheDocument();
    expect(screen.getByText('Teams')).toBeInTheDocument();
    expect(screen.getByText('Agents')).toBeInTheDocument();
    expect(screen.getByText('Reports')).toBeInTheDocument();
    expect(screen.getByText('Import')).toBeInTheDocument();
    expect(screen.getByText('Settings')).toBeInTheDocument();
  });

  it('should hide nav labels when collapsed', () => {
    useUiStore.setState({ sidebarCollapsed: true });
    renderSidebar();
    expect(screen.queryByText('Dashboard')).not.toBeInTheDocument();
    expect(screen.queryByText('Teams')).not.toBeInTheDocument();
    expect(screen.queryByText('Agents')).not.toBeInTheDocument();
  });

  it('should toggle sidebar when toggle button is clicked', () => {
    renderSidebar();

    expect(useUiStore.getState().sidebarCollapsed).toBe(false);

    // The toggle button is the last button in the sidebar
    const toggleButton = screen.getByRole('button');
    fireEvent.click(toggleButton);

    expect(useUiStore.getState().sidebarCollapsed).toBe(true);
  });

  it('should render navigation links with correct hrefs', () => {
    renderSidebar();

    const dashboardLink = screen.getByText('Dashboard').closest('a');
    expect(dashboardLink).toHaveAttribute('href', '/');

    const teamsLink = screen.getByText('Teams').closest('a');
    expect(teamsLink).toHaveAttribute('href', '/teams');

    const agentsLink = screen.getByText('Agents').closest('a');
    expect(agentsLink).toHaveAttribute('href', '/agents');

    const reportsLink = screen.getByText('Reports').closest('a');
    expect(reportsLink).toHaveAttribute('href', '/reports');

    const settingsLink = screen.getByText('Settings').closest('a');
    expect(settingsLink).toHaveAttribute('href', '/settings');
  });

  it('should have the correct width class when expanded', () => {
    renderSidebar();
    const aside = document.querySelector('aside');
    expect(aside?.className).toContain('w-[240px]');
  });

  it('should have the correct width class when collapsed', () => {
    useUiStore.setState({ sidebarCollapsed: true });
    renderSidebar();
    const aside = document.querySelector('aside');
    expect(aside?.className).toContain('w-[60px]');
  });
});
