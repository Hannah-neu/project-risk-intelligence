import {
  FolderKanban,
  LayoutDashboard,
  RefreshCw,
  Sparkles
} from "lucide-react";
import { NavLink } from "react-router-dom";

const navItems = [
  { label: "Overview", to: "/", icon: LayoutDashboard },
  { label: "Projects", to: "/projects", icon: FolderKanban },
  { label: "AI Insights", to: "/ai-insights", icon: Sparkles },
  { label: "Data Sync", to: "/data-sync", icon: RefreshCw }
];

export function Sidebar() {
  return (
    <aside className="sidebar">
      <div>
        <div className="brand">
          <img className="brand-logo" src="/app-icon.png" alt="" aria-hidden="true" />
          <div className="brand-name">Project Risk<br />Intelligence</div>
        </div>

        <nav className="nav-list" aria-label="Primary navigation">
          {navItems.map(({ label, to, icon: Icon }) => (
            <NavLink
              key={to}
              to={to}
              end={to === "/"}
              className={({ isActive }) => `nav-item${isActive ? " active" : ""}`}
            >
              <Icon size={20} strokeWidth={1.8} />
              <span>{label}</span>
            </NavLink>
          ))}
        </nav>
      </div>

      <div className="sidebar-footer">
        <div className="footer-rule" />
        <p>Project Risk Intelligence</p>
        <span>Powered by AI</span>
        <span>Built for PMOs</span>
      </div>
    </aside>
  );
}
