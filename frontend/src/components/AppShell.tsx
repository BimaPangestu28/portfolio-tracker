/**
 * AppShell — Phase 5B calm fintech shell
 *
 * Desktop: collapsible sidebar + sticky topbar
 * Mobile:  hamburger → sheet-left + bottom 4-tab nav
 *
 * 6-item IA: Dashboard · Portofolio · Rencana · Budget · Data · Chat
 */

import { useState } from "react";
import { NavLink, Outlet, useLocation } from "react-router-dom";
import {
  LayoutDashboard,
  Wallet,
  LineChart,
  Target,
  Banknote,
  Inbox,
  MessageSquare,
  MessageCircle,
  Send,
  Menu,
  X,
  RefreshCw,
  Sun,
  Moon,
  PieChart,
  PanelLeft,
  Sparkles,
  Lock,
} from "lucide-react";
import { useTheme } from "@/components/theme-provider";
import { useRefreshPrices } from "@/api/hooks";
import { useAuth } from "@/auth/AuthContext";
import { cn } from "@/lib/utils";
import { toast } from "sonner";

/* ── Nav items ──────────────────────────────────────────────────────── */

interface NavItem {
  to: string;
  label: string;
  icon: React.ElementType;
  end?: boolean;
}

const NAV_ITEMS: NavItem[] = [
  { to: "/",          label: "Dashboard",  icon: LayoutDashboard, end: true },
  { to: "/portfolio", label: "Portofolio", icon: Wallet },
  { to: "/performance", label: "Performa",   icon: LineChart },
  { to: "/planner",   label: "Rencana",    icon: Target },
  { to: "/budget",    label: "Budget",     icon: Banknote },
  { to: "/data",      label: "Data",       icon: Inbox },
  { to: "/chat",      label: "Chat",       icon: MessageSquare },
  { to: "/whatsapp", label: "WhatsApp",   icon: MessageCircle },
  { to: "/telegram",  label: "Telegram",   icon: Send },
];

/** Bottom nav shows first 4 + "More" (opens sheet) */
const BOTTOM_KEYS = ["/", "/portfolio", "/budget", "/chat"];

/* ── Small primitives ───────────────────────────────────────────────── */

function IconBtn({
  onClick,
  title,
  children,
  className,
}: {
  onClick?: () => void;
  title?: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <button type="button" className={cn("pt-icon-btn", className)} onClick={onClick} title={title}>
      {children}
    </button>
  );
}

/* ── Sidebar nav list ───────────────────────────────────────────────── */

function NavList({ collapsed, onNavigate }: { collapsed?: boolean; onNavigate?: () => void }) {
  const location = useLocation();

  function isActive(item: NavItem) {
    if (item.end) return location.pathname === item.to;
    return location.pathname.startsWith(item.to);
  }

  return (
    <nav className="pt-nav">
      {NAV_ITEMS.map((item) => {
        const active = isActive(item);
        return (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.end}
            title={collapsed ? item.label : undefined}
            className={cn("pt-nav-item", active && "active")}
            onClick={onNavigate}
          >
            <item.icon size={18} strokeWidth={active ? 2.2 : 1.8} />
            <span className="pt-nav-label">{item.label}</span>
          </NavLink>
        );
      })}
    </nav>
  );
}

/* ── Full sidebar (desktop) ─────────────────────────────────────────── */

function Sidebar({ collapsed, onCollapse }: { collapsed: boolean; onCollapse: () => void }) {
  const { lock } = useAuth();
  return (
    <aside className={cn("pt-sidebar", collapsed && "collapsed")}>
      {/* Brand */}
      <div className="pt-brand">
        <div className="pt-brand-mark">
          <PieChart size={18} strokeWidth={2} />
        </div>
        <span className="pt-brand-name pt-nav-label">Portfolio</span>
      </div>

      {/* Nav */}
      <NavList collapsed={collapsed} />

      {/* Footer */}
      <div className="pt-sidebar-foot">
        <button
          type="button"
          className="pt-nav-item"
          onClick={lock}
          title="Kunci"
        >
          <Lock size={18} strokeWidth={1.8} />
          <span className="pt-nav-label">Kunci portofolio</span>
        </button>
        <button
          type="button"
          className="pt-nav-item"
          onClick={onCollapse}
          title={collapsed ? "Perluas" : "Ciutkan"}
        >
          <PanelLeft size={18} strokeWidth={1.8} />
          <span className="pt-nav-label">Ciutkan</span>
        </button>
        <div
          className="pt-nav-label"
          style={{ padding: "8px 12px 2px", fontSize: 11, color: "hsl(var(--muted-foreground))" }}
        >
          © 2026 catalystlabs.id
        </div>
      </div>
    </aside>
  );
}

/* ── Mobile sheet ───────────────────────────────────────────────────── */

function MobileSheet({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { lock } = useAuth();
  if (!open) return null;

  function handleLock() {
    onClose();
    lock();
  }

  return (
    <>
      <div className="pt-scrim" onClick={onClose} />
      <div className="pt-sheet-left">
        <div className="pt-brand">
          <div className="pt-brand-mark">
            <PieChart size={18} strokeWidth={2} />
          </div>
          <span className="pt-brand-name">Portfolio</span>
          <IconBtn className="ml-auto" onClick={onClose} title="Tutup">
            <X size={18} />
          </IconBtn>
        </div>
        <NavList onNavigate={onClose} />
        <div className="pt-sidebar-foot">
          <button
            type="button"
            className="pt-nav-item"
            onClick={handleLock}
            title="Kunci"
          >
            <Lock size={18} strokeWidth={1.8} />
            <span>Kunci portofolio</span>
          </button>
          <div
            style={{ padding: "8px 12px 2px", fontSize: 11, color: "hsl(var(--muted-foreground))" }}
          >
            © 2026 catalystlabs.id
          </div>
        </div>
      </div>
    </>
  );
}

/* ── Mobile bottom nav ──────────────────────────────────────────────── */

function BottomNav({ onMore }: { onMore: () => void }) {
  const location = useLocation();
  const isMore = !BOTTOM_KEYS.includes(location.pathname) && location.pathname !== "/";

  return (
    <nav className="pt-bottom-nav">
      {BOTTOM_KEYS.map((to) => {
        const item = NAV_ITEMS.find((n) => n.to === to)!;
        const active = item.end
          ? location.pathname === to
          : location.pathname.startsWith(to);
        return (
          <NavLink
            key={to}
            to={to}
            end={item.end}
            className={cn("pt-tab", active && "active")}
          >
            <item.icon size={20} />
            <span>{item.label}</span>
          </NavLink>
        );
      })}
      <button
        type="button"
        className={cn("pt-tab", isMore && "active")}
        onClick={onMore}
      >
        <Menu size={20} />
        <span>Lainnya</span>
      </button>
    </nav>
  );
}

/* ── Page title from current route ─────────────────────────────────── */

function usePageTitle() {
  const location = useLocation();
  const item = NAV_ITEMS.find((n) => {
    if (n.end) return location.pathname === n.to;
    return location.pathname.startsWith(n.to);
  });
  return item?.label ?? "Portfolio";
}

/* ── Topbar ─────────────────────────────────────────────────────────── */

function Topbar({ onHamburger }: { onHamburger: () => void }) {
  const title = usePageTitle();
  const { theme, setTheme } = useTheme();
  const refresh = useRefreshPrices();

  return (
    <header className="pt-topbar">
      <IconBtn className="pt-hamburger" onClick={onHamburger} title="Menu">
        <Menu size={20} />
      </IconBtn>

      <div
        className="flex-1 truncate"
        style={{ fontWeight: 600, fontSize: 17, letterSpacing: "-0.015em" }}
      >
        {title}
      </div>

      <IconBtn
        onClick={() =>
          refresh.mutate(undefined, {
            onSuccess: () => toast.success("Harga diperbarui"),
            onError: (err) => toast.error((err as Error).message),
          })
        }
        title="Perbarui harga"
        className={refresh.isPending ? "animate-spin" : ""}
      >
        <RefreshCw size={16} />
      </IconBtn>

      <IconBtn
        onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
        title={theme === "dark" ? "Mode terang" : "Mode gelap"}
      >
        {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
      </IconBtn>

      <NavLink
        to="/chat"
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 6,
          height: 32,
          padding: "0 12px",
          borderRadius: "var(--radius-sm)",
          fontSize: 13,
          fontWeight: 500,
          border: "1px solid hsl(var(--border-strong))",
          background: "hsl(var(--card))",
          color: "hsl(var(--foreground))",
          textDecoration: "none",
          marginLeft: 4,
          whiteSpace: "nowrap",
        }}
      >
        <Sparkles size={14} />
        Tanya
      </NavLink>
    </header>
  );
}

/* ── AppShell ───────────────────────────────────────────────────────── */

export default function AppShell() {
  const [collapsed, setCollapsed] = useState(false);
  const [sheet, setSheet] = useState(false);

  return (
    <div className="pt-shell">
      {/* Desktop sidebar */}
      <Sidebar collapsed={collapsed} onCollapse={() => setCollapsed((c) => !c)} />

      {/* Mobile sheet + scrim */}
      <MobileSheet open={sheet} onClose={() => setSheet(false)} />

      {/* Main area */}
      <div className="pt-main">
        <Topbar onHamburger={() => setSheet(true)} />
        <div className="pt-page-scroll">
          <div className="pt-page">
            <Outlet />
          </div>
        </div>
      </div>

      {/* Mobile bottom nav */}
      <BottomNav onMore={() => setSheet(true)} />
    </div>
  );
}
