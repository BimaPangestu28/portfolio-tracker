import { NavLink, Outlet } from "react-router-dom";

const links = [
  { to: "/", label: "Dashboard", end: true },
  { to: "/holdings", label: "Holdings" },
  { to: "/transactions", label: "Transactions" },
  { to: "/planner", label: "Planner" },
  { to: "/settings", label: "Settings" },
  { to: "/import", label: "Import" },
  { to: "/budget", label: "Budget" },
];

export default function Layout() {
  return (
    <div className="min-h-screen bg-gray-50 text-gray-900">
      <header className="border-b bg-white">
        <nav className="mx-auto flex max-w-5xl gap-4 px-4 py-3">
          <span className="font-bold"><span aria-hidden="true">📊</span> Portfolio</span>
          {links.map((l) => (
            <NavLink
              key={l.to}
              to={l.to}
              end={l.end}
              className={({ isActive }) => (isActive ? "font-semibold text-blue-600" : "text-gray-600")}
            >
              {l.label}
            </NavLink>
          ))}
        </nav>
      </header>
      <main className="mx-auto max-w-5xl p-4">
        <Outlet />
      </main>
    </div>
  );
}
