import type { PropsWithChildren } from "react";
import { NavLink } from "react-router-dom";

const navItems = [
  { to: "/posture", label: "Posture" },
  { to: "/inventory", label: "Inventory" },
  { to: "/blocked", label: "Blocked" },
  { to: "/qkd", label: "QKD links" },
];

export function AppShell({ children }: PropsWithChildren) {
  return (
    <div className="min-h-screen flex flex-col">
      <header className="bg-ink-900 text-white">
        <div className="max-w-7xl mx-auto px-6 py-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <span className="font-mono text-lg font-bold tracking-tight">
              Sezar
            </span>
            <span className="text-xs text-ink-400">
              quantum-risk posture (v0.1 dashboard)
            </span>
          </div>
          <nav className="flex gap-1">
            {navItems.map((it) => (
              <NavLink
                key={it.to}
                to={it.to}
                className={({ isActive }) =>
                  `nav-link ${isActive ? "nav-link-active" : ""}`
                }
              >
                {it.label}
              </NavLink>
            ))}
          </nav>
        </div>
      </header>
      <main className="flex-1 max-w-7xl mx-auto w-full px-6 py-8">
        {children}
      </main>
      <footer className="border-t border-ink-200 bg-white">
        <div className="max-w-7xl mx-auto px-6 py-3 text-xs text-ink-600 flex justify-between">
          <span>
            schema v1.1 ·{" "}
            <a
              className="underline hover:text-ink-900"
              href="https://github.com/e2esolutions-tech/sezar"
            >
              github.com/e2esolutions-tech/sezar
            </a>
          </span>
          <span className="font-mono">
            three axes: A · C · G
          </span>
        </div>
      </footer>
    </div>
  );
}
