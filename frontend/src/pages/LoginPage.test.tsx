import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { useAuthStore } from '@/stores/auth';
import React from 'react';

// Mock the API client
vi.mock('@/api/client', () => ({
  default: {
    post: vi.fn(),
    get: vi.fn(),
  },
}));

import client from '@/api/client';

const mockNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual('react-router-dom');
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

describe('LoginPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAuthStore.setState({ token: null, user: null });
    // Default: local auth only, no SSO
    (client.get as ReturnType<typeof vi.fn>).mockResolvedValue({
      data: { local: true, oidc: false, saml: false },
    });
  });

  it('should render login form', async () => {
    const { LoginPage } = await import('./LoginPage');
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );

    expect(screen.getByText('AppControl')).toBeInTheDocument();
    expect(screen.getByText('Sign in to your account')).toBeInTheDocument();
    expect(screen.getByLabelText('Email')).toBeInTheDocument();
    expect(screen.getByLabelText('Password')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeInTheDocument();
  });

  it('should not show SSO button when not configured', async () => {
    const { LoginPage } = await import('./LoginPage');
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );

    // Wait for auth/info to load
    await waitFor(() => {
      expect(client.get).toHaveBeenCalledWith('/auth/info');
    });

    expect(screen.queryByRole('button', { name: 'Sign in with SSO' })).not.toBeInTheDocument();
  });

  it('should show SSO button when OIDC is configured', async () => {
    (client.get as ReturnType<typeof vi.fn>).mockResolvedValue({
      data: { local: true, oidc: true, saml: false },
    });

    const { LoginPage } = await import('./LoginPage');
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Sign in with SSO' })).toBeInTheDocument();
    });
  });

  it('should show SSO button when SAML is configured', async () => {
    (client.get as ReturnType<typeof vi.fn>).mockResolvedValue({
      data: { local: true, oidc: false, saml: true },
    });

    const { LoginPage } = await import('./LoginPage');
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Sign in with SSO' })).toBeInTheDocument();
    });
  });

  it('should update email and password fields', async () => {
    const { LoginPage } = await import('./LoginPage');
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );

    const emailInput = screen.getByLabelText('Email');
    const passwordInput = screen.getByLabelText('Password');

    fireEvent.change(emailInput, { target: { value: 'admin@example.com' } });
    fireEvent.change(passwordInput, { target: { value: 'secret123' } });

    expect(emailInput).toHaveValue('admin@example.com');
    expect(passwordInput).toHaveValue('secret123');
  });

  it('should call API and navigate on successful login', async () => {
    const mockUser = { id: '1', email: 'admin@example.com', name: 'Admin', org_id: 'org-1', role: 'admin' };
    (client.post as ReturnType<typeof vi.fn>).mockResolvedValue({
      data: { token: 'jwt-token', user: mockUser },
    });

    const { LoginPage } = await import('./LoginPage');
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );

    fireEvent.change(screen.getByLabelText('Email'), { target: { value: 'admin@example.com' } });
    fireEvent.change(screen.getByLabelText('Password'), { target: { value: 'password' } });
    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));

    await waitFor(() => {
      expect(client.post).toHaveBeenCalledWith('/auth/login', {
        email: 'admin@example.com',
        password: 'password',
      });
    });

    await waitFor(() => {
      expect(mockNavigate).toHaveBeenCalledWith('/');
    });

    expect(useAuthStore.getState().token).toBe('jwt-token');
    expect(useAuthStore.getState().user).toEqual(mockUser);
  });

  it('should show error message on failed login', async () => {
    (client.post as ReturnType<typeof vi.fn>).mockRejectedValue({
      response: { data: { message: 'Invalid credentials' } },
    });

    const { LoginPage } = await import('./LoginPage');
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );

    fireEvent.change(screen.getByLabelText('Email'), { target: { value: 'bad@example.com' } });
    fireEvent.change(screen.getByLabelText('Password'), { target: { value: 'wrong' } });
    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));

    await waitFor(() => {
      expect(screen.getByText('Invalid credentials')).toBeInTheDocument();
    });
  });

  it('should show generic error when no error message from server', async () => {
    (client.post as ReturnType<typeof vi.fn>).mockRejectedValue({
      response: { data: {} },
    });

    const { LoginPage } = await import('./LoginPage');
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );

    fireEvent.change(screen.getByLabelText('Email'), { target: { value: 'bad@example.com' } });
    fireEvent.change(screen.getByLabelText('Password'), { target: { value: 'wrong' } });
    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));

    await waitFor(() => {
      expect(screen.getByText('Login failed')).toBeInTheDocument();
    });
  });

  it('should show "Signing in..." text while loading', async () => {
    // Make the post never resolve
    (client.post as ReturnType<typeof vi.fn>).mockReturnValue(new Promise(() => {}));

    const { LoginPage } = await import('./LoginPage');
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );

    fireEvent.change(screen.getByLabelText('Email'), { target: { value: 'admin@example.com' } });
    fireEvent.change(screen.getByLabelText('Password'), { target: { value: 'password' } });
    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));

    await waitFor(() => {
      expect(screen.getByText('Signing in...')).toBeInTheDocument();
    });
  });

  it('should disable submit button while loading', async () => {
    (client.post as ReturnType<typeof vi.fn>).mockReturnValue(new Promise(() => {}));

    const { LoginPage } = await import('./LoginPage');
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );

    fireEvent.change(screen.getByLabelText('Email'), { target: { value: 'admin@example.com' } });
    fireEvent.change(screen.getByLabelText('Password'), { target: { value: 'password' } });
    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));

    await waitFor(() => {
      expect(screen.getByText('Signing in...').closest('button')).toBeDisabled();
    });
  });

  it('should show error on network error (no response)', async () => {
    (client.post as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('Network Error'));

    const { LoginPage } = await import('./LoginPage');
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );

    fireEvent.change(screen.getByLabelText('Email'), { target: { value: 'a@b.com' } });
    fireEvent.change(screen.getByLabelText('Password'), { target: { value: 'pw' } });
    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));

    await waitFor(() => {
      expect(screen.getByText('Login failed')).toBeInTheDocument();
    });
  });

  it('should handle auth/info API failure gracefully', async () => {
    (client.get as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('Network Error'));

    const { LoginPage } = await import('./LoginPage');
    render(
      <MemoryRouter>
        <LoginPage />
      </MemoryRouter>,
    );

    // Form should still work
    expect(screen.getByLabelText('Email')).toBeInTheDocument();
    expect(screen.getByLabelText('Password')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeInTheDocument();
  });
});
