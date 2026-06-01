import { NavLink, Outlet, useLocation } from "react-router-dom";
import { LayoutDashboard, Wallet, ArrowLeftRight, Target, Settings, Upload, PiggyBank, Plug, MessageSquare } from "lucide-react";
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { Separator } from "@/components/ui/separator";
import { ModeToggle } from "@/components/mode-toggle";

const links = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard, end: true },
  { to: "/holdings", label: "Holdings", icon: Wallet, end: false },
  { to: "/transactions", label: "Transactions", icon: ArrowLeftRight, end: false },
  { to: "/planner", label: "Planner", icon: Target, end: false },
  { to: "/budget", label: "Budget", icon: PiggyBank, end: false },
  { to: "/import", label: "Import", icon: Upload, end: false },
  { to: "/connectors", label: "Connectors", icon: Plug, end: false },
  { to: "/chat", label: "Chat", icon: MessageSquare, end: false },
  { to: "/settings", label: "Settings", icon: Settings, end: false },
];

export default function AppLayout() {
  const location = useLocation();
  const current = links.find((l) =>
    l.end ? location.pathname === l.to : location.pathname.startsWith(l.to),
  );

  return (
    <SidebarProvider>
      <Sidebar>
        <SidebarHeader>
          <div className="flex items-center gap-2 px-2 py-1.5 text-base font-semibold">
            <span aria-hidden="true">📊</span> Portfolio
          </div>
        </SidebarHeader>
        <SidebarContent>
          <SidebarGroup>
            <SidebarGroupContent>
              <SidebarMenu>
                {links.map((l) => (
                  <SidebarMenuItem key={l.to}>
                    <SidebarMenuButton asChild tooltip={l.label}>
                      <NavLink to={l.to} end={l.end}>
                        <l.icon />
                        <span>{l.label}</span>
                      </NavLink>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>
      </Sidebar>
      <SidebarInset>
        <header className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger className="-ml-1" />
          <Separator orientation="vertical" className="mr-2 h-4" />
          <h1 className="text-sm font-semibold">{current?.label ?? "Portfolio"}</h1>
          <div className="ml-auto">
            <ModeToggle />
          </div>
        </header>
        <main className="flex-1 p-4 md:p-6">
          <Outlet />
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}
