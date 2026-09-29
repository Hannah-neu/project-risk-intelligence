import { lazy, Suspense } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { AppLayout } from "./components/layout/AppLayout";
import { Dashboard } from "./pages/Dashboard";
import { DataSync } from "./pages/DataSync";
import { Projects } from "./pages/Projects";
import { ProjectDetail } from "./pages/ProjectDetail";
import { Settings } from "./pages/Settings";

const AIInsights = lazy(() => import("./pages/AIInsights").then((module) => ({ default: module.AIInsights })));

export default function App() {
  return (
    <Routes>
      <Route element={<AppLayout />}>
        <Route index element={<Dashboard />} />
        <Route path="projects" element={<Projects />} />
        <Route path="projects/:id" element={<ProjectDetail />} />
        <Route path="risks" element={<Navigate to="/" replace />} />
        <Route path="ai-insights" element={<Suspense fallback={<div className="route-loading">Loading portfolio insights…</div>}><AIInsights /></Suspense>} />
        <Route path="data-sync" element={<DataSync />} />
        <Route path="settings" element={<Settings />} />
      </Route>
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  );
}
