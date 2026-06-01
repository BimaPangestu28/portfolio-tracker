import { useState } from "react";
import { Trash2 } from "lucide-react";
import {
  useAccounts,
  useConnectors,
  useCreateConnector,
  useDeleteConnector,
  useSyncConnector,
} from "../api/hooks";
import { QueryState } from "../components/QueryState";
import type { SyncReport } from "../api/schemas";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";

const nativeSelect =
  "flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring";

const EMPTY_FORM = {
  account_id: "",
  label: "",
  address: "",
  base_url: "",
  api_key: "",
};

export default function ConnectorsPage() {
  const accounts = useAccounts();
  const connectors = useConnectors();
  const createConnector = useCreateConnector();
  const deleteConnector = useDeleteConnector();
  const syncConnector = useSyncConnector();

  const [form, setForm] = useState(EMPTY_FORM);
  const [syncResults, setSyncResults] = useState<Record<number, SyncReport>>({});

  const set =
    (k: keyof typeof EMPTY_FORM) =>
    (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
      setForm({ ...form, [k]: e.target.value });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const config = JSON.stringify({
      address: form.address,
      base_url: form.base_url || undefined,
      api_key: form.api_key || undefined,
      native_symbol: "ETH",
    });
    createConnector.mutate({
      account_id: Number(form.account_id),
      kind: "evm_wallet",
      label: form.label,
      config_json: config,
    });
    setForm(EMPTY_FORM);
  };

  const handleSync = (id: number) => {
    syncConnector.mutate(id, {
      onSuccess: (report) => {
        setSyncResults((prev) => ({ ...prev, [id]: report }));
      },
    });
  };

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Connectors</h1>

      <Card>
        <CardHeader>
          <CardTitle>Add EVM Wallet Connector</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <select
              aria-label="Account"
              className={nativeSelect}
              value={form.account_id}
              onChange={set("account_id")}
              required
            >
              <option value="">Account…</option>
              {(accounts.data ?? []).map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
            </select>
            <Input
              aria-label="Connector label"
              placeholder="Label (e.g. My ETH Wallet)"
              value={form.label}
              onChange={set("label")}
              required
            />
            <Input
              aria-label="Wallet address"
              placeholder="0x… wallet address"
              value={form.address}
              onChange={set("address")}
              required
            />
            <Input
              aria-label="Explorer base URL"
              placeholder="Explorer base URL (optional)"
              value={form.base_url}
              onChange={set("base_url")}
            />
            <Input
              aria-label="API key"
              type="password"
              placeholder="API key (optional)"
              value={form.api_key}
              onChange={set("api_key")}
            />
            <div className="flex items-center gap-3 sm:col-span-2">
              <Button type="submit" disabled={createConnector.isPending}>
                {createConnector.isPending ? "Adding…" : "Add connector"}
              </Button>
              {createConnector.error && (
                <span className="text-sm text-destructive">{(createConnector.error as Error).message}</span>
              )}
            </div>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Connectors</CardTitle>
        </CardHeader>
        <CardContent>
          <QueryState isLoading={connectors.isLoading} error={connectors.error}>
            <ul className="space-y-2">
              {(connectors.data ?? []).length === 0 && (
                <li className="rounded-lg border bg-card p-4 text-sm text-muted-foreground">
                  No connectors yet. Add an EVM wallet above.
                </li>
              )}
              {(connectors.data ?? []).map((c) => (
                <li key={c.id} className="flex flex-wrap items-center gap-3 rounded-lg border bg-card p-3 text-sm">
                  <span className="font-medium">{c.label}</span>
                  <Badge variant="secondary">{c.kind}</Badge>
                  <span className="text-muted-foreground">
                    {c.last_synced_at
                      ? `Last synced: ${c.last_synced_at.slice(0, 19).replace("T", " ")}`
                      : "Never synced"}
                  </span>
                  {syncResults[c.id] && (
                    <span className="text-xs text-emerald-600 dark:text-emerald-400">
                      inserted: {syncResults[c.id].inserted} · staged: {syncResults[c.id].staged} · skipped: {syncResults[c.id].skipped}
                    </span>
                  )}
                  <div className="ml-auto flex items-center gap-2">
                    <Button
                      type="button"
                      size="sm"
                      aria-label={`Sync ${c.label}`}
                      onClick={() => handleSync(c.id)}
                      disabled={syncConnector.isPending}
                    >
                      Sync now
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      aria-label={`Delete ${c.label}`}
                      className="text-destructive hover:text-destructive"
                      onClick={() => deleteConnector.mutate(c.id)}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          </QueryState>
        </CardContent>
      </Card>
    </div>
  );
}
