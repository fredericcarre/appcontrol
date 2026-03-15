import { Routes, Route, Navigate } from 'react-router-dom';
import { useAuthStore } from '@/stores/auth';
import { useWebSocket } from '@/hooks/use-websocket';
import { Sidebar } from '@/components/layout/Sidebar';
import { Header } from '@/components/layout/Header';
import { ErrorBoundary } from '@/components/ErrorBoundary';
import { TooltipProvider } from '@/components/ui/tooltip';
import { DashboardPage } from '@/pages/DashboardPage';
import { MapViewPage } from '@/pages/MapViewPage';
import { TeamsPage } from '@/pages/TeamsPage';
import { AgentsPage } from '@/pages/AgentsPage';
import { GatewaysPage } from '@/pages/GatewaysPage';
import { ReportsPage } from '@/pages/ReportsPage';
import { SettingsPage } from '@/pages/SettingsPage';
import { OnboardingPage } from '@/pages/OnboardingPage';
import { LoginPage } from '@/pages/LoginPage';
import ImportPage from '@/pages/ImportPage';
import { EnrollmentTokensPage } from '@/pages/EnrollmentTokens';
import { ShareLinkPage } from '@/pages/ShareLinkPage';
import { ApiKeysPage } from '@/pages/ApiKeysPage';
import { DiscoveryPage } from '@/pages/DiscoveryPage';
import { UsersPage } from '@/pages/UsersPage';
import { SupervisionPage } from '@/pages/SupervisionPage';
import { useUiStore } from '@/stores/ui';
import { cn } from '@/lib/utils';

function AuthLayout({ children }: { children: React.ReactNode }) {
  const sidebarCollapsed = useUiStore((s) => s.sidebarCollapsed);

  // Initialize WebSocket connection for real-time updates
  useWebSocket();

  return (
    <div className="flex h-screen overflow-hidden">
      <Sidebar />
      <div className={cn("flex flex-col flex-1 overflow-hidden transition-all duration-200", sidebarCollapsed ? "ml-[60px]" : "ml-[240px]")}>
        <Header />
        <main className="flex-1 overflow-auto p-6">
          <ErrorBoundary>
            {children}
          </ErrorBoundary>
        </main>
      </div>
    </div>
  );
}

export default function App() {
  // Check user (persisted to localStorage) rather than token (kept in memory).
  // Browser auth uses HttpOnly cookies — the token is sent automatically.
  // The in-memory token is only for API key / CLI flows.
  const user = useAuthStore((s) => s.user);

  if (!user) {
    return (
      <TooltipProvider delayDuration={300}>
        <Routes>
          <Route path="/login" element={<LoginPage />} />
          <Route path="/share/:token" element={<ShareLinkPage />} />
          <Route path="*" element={<Navigate to="/login" replace />} />
        </Routes>
      </TooltipProvider>
    );
  }

  return (
    <TooltipProvider delayDuration={300}>
      <Routes>
        <Route path="/" element={<AuthLayout><DashboardPage /></AuthLayout>} />
        <Route path="/discovery" element={<AuthLayout><DiscoveryPage /></AuthLayout>} />
        <Route path="/apps/:appId" element={<AuthLayout><MapViewPage /></AuthLayout>} />
        <Route path="/teams" element={<AuthLayout><TeamsPage /></AuthLayout>} />
        <Route path="/users" element={<AuthLayout><UsersPage /></AuthLayout>} />
        <Route path="/gateways" element={<AuthLayout><GatewaysPage /></AuthLayout>} />
        <Route path="/agents" element={<AuthLayout><AgentsPage /></AuthLayout>} />
        <Route path="/reports" element={<AuthLayout><ReportsPage /></AuthLayout>} />
        <Route path="/settings" element={<AuthLayout><SettingsPage /></AuthLayout>} />
        <Route path="/onboarding" element={<AuthLayout><OnboardingPage /></AuthLayout>} />
        <Route path="/import" element={<AuthLayout><ImportPage /></AuthLayout>} />
        <Route path="/enrollment" element={<AuthLayout><EnrollmentTokensPage /></AuthLayout>} />
        <Route path="/settings/api-keys" element={<AuthLayout><ApiKeysPage /></AuthLayout>} />
        <Route path="/share/:token" element={<ShareLinkPage />} />
        <Route path="/supervision" element={<SupervisionPage />} />
        <Route path="/login" element={<Navigate to="/" replace />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </TooltipProvider>
  );
}
