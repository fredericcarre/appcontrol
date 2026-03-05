import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Breadcrumbs } from './Breadcrumb';

function renderBreadcrumbs(route: string) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={[route]}>
        <Breadcrumbs />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('Breadcrumbs', () => {
  it('should render home icon link', () => {
    renderBreadcrumbs('/');
    const homeLink = screen.getByRole('link');
    expect(homeLink).toHaveAttribute('href', '/');
  });

  it('should render no breadcrumb segments on root path', () => {
    renderBreadcrumbs('/');
    // Only the home link should be rendered
    const links = screen.getAllByRole('link');
    expect(links).toHaveLength(1);
  });

  it('should render breadcrumb for /teams', () => {
    renderBreadcrumbs('/teams');
    expect(screen.getByText('Teams')).toBeInTheDocument();
  });

  it('should render breadcrumb for /agents', () => {
    renderBreadcrumbs('/agents');
    expect(screen.getByText('Agents')).toBeInTheDocument();
  });

  it('should render breadcrumb for /reports', () => {
    renderBreadcrumbs('/reports');
    expect(screen.getByText('Reports')).toBeInTheDocument();
  });

  it('should render breadcrumb for /settings', () => {
    renderBreadcrumbs('/settings');
    expect(screen.getByText('Settings')).toBeInTheDocument();
  });

  it('should render breadcrumb for /onboarding', () => {
    renderBreadcrumbs('/onboarding');
    expect(screen.getByText('Onboarding')).toBeInTheDocument();
  });

  it('should render nested breadcrumbs for /apps/my-app-id', () => {
    renderBreadcrumbs('/apps/my-app-id');
    expect(screen.getByText('Applications')).toBeInTheDocument();
    expect(screen.getByText('my-app-id')).toBeInTheDocument();
  });

  it('should make intermediate segments clickable links', () => {
    renderBreadcrumbs('/apps/my-app-id');
    // "Applications" should be a link, "my-app-id" should be plain text (last segment)
    const appLink = screen.getByText('Applications');
    expect(appLink.tagName).toBe('A');
    expect(appLink).toHaveAttribute('href', '/apps');
  });

  it('should make the last segment non-clickable', () => {
    renderBreadcrumbs('/apps/my-app-id');
    const lastSegment = screen.getByText('my-app-id');
    expect(lastSegment.tagName).toBe('SPAN');
  });

  it('should use raw segment text for unknown routes', () => {
    renderBreadcrumbs('/unknown-route');
    expect(screen.getByText('unknown-route')).toBeInTheDocument();
  });

  it('should render a single segment as the last (non-link) segment', () => {
    renderBreadcrumbs('/teams');
    const teamsText = screen.getByText('Teams');
    // It should be a span (last element), not a link
    expect(teamsText.tagName).toBe('SPAN');
    expect(teamsText.className).toContain('font-medium');
  });
});
