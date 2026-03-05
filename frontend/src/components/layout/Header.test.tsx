import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useAuthStore } from '@/stores/auth';
import { useUiStore } from '@/stores/ui';
import { useWebSocketStore } from '@/stores/websocket';
import { Header } from './Header';

function renderHeader(route = '/') {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={[route]}>
        <Header />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('Header', () => {
  beforeEach(() => {
    useAuthStore.setState({
      token: 'test-token',
      user: { id: '1', email: 'admin@example.com', name: 'John Doe', org_id: 'org-1', role: 'admin' },
    });
    useUiStore.setState({ theme: 'light', sidebarCollapsed: false, commandPaletteOpen: false });
    useWebSocketStore.setState({ connected: true, messages: [], subscribedApps: new Set() });
  });

  it('should render the header', () => {
    renderHeader();
    expect(screen.getByRole('banner')).toBeInTheDocument();
  });

  it('should display the user name', () => {
    renderHeader();
    expect(screen.getByText('John Doe')).toBeInTheDocument();
  });

  it('should display user initials in avatar', () => {
    renderHeader();
    expect(screen.getByText('JD')).toBeInTheDocument();
  });

  it('should display single letter initials for single name', () => {
    useAuthStore.setState({
      token: 'test-token',
      user: { id: '1', email: 'a@b.com', name: 'Alice', org_id: 'org-1', role: 'admin' },
    });
    renderHeader();
    expect(screen.getByText('A')).toBeInTheDocument();
  });

  it('should display ? when no user', () => {
    useAuthStore.setState({
      token: 'test-token',
      user: null,
    });
    renderHeader();
    expect(screen.getByText('?')).toBeInTheDocument();
  });

  it('should display email-based initials when user has no name', () => {
    useAuthStore.setState({
      token: 'test-token',
      user: { id: '1', email: 'john.doe@example.com', name: undefined, org_id: 'org-1', role: 'admin' },
    });
    renderHeader();
    // Should extract 'john.doe' from email, split by '.', and show 'JD' initials
    expect(screen.getByText('JD')).toBeInTheDocument();
  });

  it('should show Connected when WebSocket is connected', () => {
    useWebSocketStore.setState({ connected: true, messages: [], subscribedApps: new Set() });
    renderHeader();
    expect(screen.getByText('Connected')).toBeInTheDocument();
  });

  it('should show Offline when WebSocket is disconnected', () => {
    useWebSocketStore.setState({ connected: false, messages: [], subscribedApps: new Set() });
    renderHeader();
    expect(screen.getByText('Offline')).toBeInTheDocument();
  });

  it('should toggle theme when theme button is clicked', () => {
    renderHeader();

    expect(useUiStore.getState().theme).toBe('light');

    // Find the theme toggle button (contains Moon icon in light mode)
    const buttons = screen.getAllByRole('button');
    // Theme toggle is one of the ghost buttons
    const themeButton = buttons.find((btn) =>
      btn.querySelector('.lucide-moon') || btn.textContent === '',
    );
    // Click the first ghost button that isn't the logout button
    if (themeButton) {
      fireEvent.click(themeButton);
    } else {
      // Just click the second button (first is likely theme, third is logout)
      fireEvent.click(buttons[0]);
    }

    // Theme should have toggled
    expect(useUiStore.getState().theme).toBe('dark');
  });

  it('should show logout confirmation dialog when logout button is clicked', () => {
    renderHeader();

    const buttons = screen.getAllByRole('button');
    // The last button should be the logout button
    const lastButton = buttons[buttons.length - 1];
    fireEvent.click(lastButton);

    // Should show confirmation dialog with confirmation text
    expect(screen.getByText(/Are you sure you want to sign out/)).toBeInTheDocument();
  });

  it('should logout when confirming in the dialog', async () => {
    renderHeader();

    const buttons = screen.getAllByRole('button');
    // The last button should be the logout button
    const lastButton = buttons[buttons.length - 1];
    fireEvent.click(lastButton);

    // Find the destructive variant button (Sign out confirm button)
    const allButtons = screen.getAllByRole('button');
    const confirmButton = allButtons.find((btn) =>
      btn.classList.contains('bg-destructive') && btn.textContent?.includes('Sign out')
    );

    if (confirmButton) {
      fireEvent.click(confirmButton);
    }

    expect(useAuthStore.getState().token).toBeNull();
    expect(useAuthStore.getState().user).toBeNull();
  });
});
