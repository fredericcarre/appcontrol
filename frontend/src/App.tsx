import { Routes, Route, Navigate } from 'react-router-dom';
import { useAuthStore } from '@/stores/auth';
import { Sidebar } from '@/components/layout/Sidebar';
import { Header } from '@/components/layout/Header';
import { DashboardPage } from '@/pages/DashboardPage';
import { MapViewPage } from '@/pages/MapViewPage';
import { TeamsPage } from '@/pages/TeamsPage';
import { AgentsPage } from '@/pages/AgentsPage';
import { ReportsPage } from '@/pages/ReportsPage';
import { SettingsPage } from '@/pages/SettingsPage';
import { OnboardingPage } from '@/pages/OnboardingPage';
import { LoginPage } from '@/pages/LoginPage';
import ImportPage from '@/pages/ImportPage';
import { useUiStore } from '@/stores/ui';
import { cn } from '@/lib/utils';

function AuthLayout({ children }: { children: React.ReactNode }) {
  const sidebarCollapsed = useUiStore((s) => s.sidebarCollapsed);

  return (
    <div className="flex h-screen overflow-hidden">
      <Sidebar />
      <div className={cn("flex flex-col flex-1 overflow-hidden transition-all duration-200", sidebarCollapsed ? "ml-[60px]" : "ml-[240px]")}>
        <Header />
        <main className="flex-1 overflow-auto p-6">
          {children}
        </main>
      </div>
    </div>
  );
}

export default function App() {
  const token = useAuthStore((s) => s.token);

  if (!token) {
    return (
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route path="*" element={<Navigate to="/login" replace />} />
      </Routes>
    );
  }

  return (
    <Routes>
      <Route path="/" element={<AuthLayout><DashboardPage /></AuthLayout>} />
      <Route path="/apps/:appId" element={<AuthLayout><MapViewPage /></AuthLayout>} />
      <Route path="/teams" element={<AuthLayout><TeamsPage /></AuthLayout>} />
      <Route path="/agents" element={<AuthLayout><AgentsPage /></AuthLayout>} />
      <Route path="/reports" element={<AuthLayout><ReportsPage /></AuthLayout>} />
      <Route path="/settings" element={<AuthLayout><SettingsPage /></AuthLayout>} />
      <Route path="/onboarding" element={<AuthLayout><OnboardingPage /></AuthLayout>} />
      <Route path="/import" element={<AuthLayout><ImportPage /></AuthLayout>} />
      <Route path="/login" element={<Navigate to="/" replace />} />
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  );
}
