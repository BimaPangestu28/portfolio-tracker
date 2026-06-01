import { useState } from "react";
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
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";

const nativeSelect =
  "flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring";

export default function ConnectorsPage() {
  const accounts = useAccounts();
  const connectors = useConnectors();
  const createConnector = useCreateConnector();
  const deleteConnector = useDeleteConnector();
  const syncConnector = useSyncConnector();

  const [form, setForm] = useState({
    account_id: "",
    label: "",
    address: "",
    base_url: "",
    api_key: "",
  });

  const [syncResults, setSyncResults] = useState<Record<number, SyncReport>>({});

  const set =
    (k: keyof typeof form) =>
    (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
      setForm({ ...form, [k]: e.target.value });

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    const config_json = JSON.stringify({
      address: form.address,
      base_url: form.base_url || undefined,
      api_key: form.api_key || undefined,
      native_symbol: "ETH",
    });
    createConnector.mutate(
      {
        account_id: Number(form.account_id),
        kind: "evm_wallet",
        label: form.label,
        config_json,
      },
      {
        onSuccess: () =>
          setForm({ account_id: "", label: "", address: "", base_url: "", api_key: "" }),
      },
    );
  };

  const handleSync = (id: number) => {
    syncConnector.mutate(id, {
      onSuccess: (report) => {
        setSyncResults((prev) => ({ ...prev, [id]: report }));
      },
    });
  };

  return (
    <div className="space-y-8">
      <h1 className="text-xl font-semibold">Connectors</h1>

      <Card>
        <CardHeader>
          <CardTitle>Add EVM Wallet Connector</CardTitle>
        </CardHeader>
        <CardContent>
          <form
            onSubmit={submit}
            className="grid grid-cols-2 gap-2 sm:grid-cols-3"
          >
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
              placeholder="Label"
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
              placeholder="API key (optional)"
              value={form.api_key}
              onChange={set("api_key")}
            />

            <Button
              type="submit"
              className="sm:col-start-3"
              disabled={createConnector.isPending}
            >
              {createConnector.isPending ? "Adding…" : "Add connector"}
            </Button>

            {createConnector.error && (
              <div className="col-span-2 text-sm text-destructive sm:col-span-3">
                {(createConnector.error as Error).message}
              </div>
            )}
          </form>
        </CardContent>
      </Card>

      <section className="space-y-2">
        <h2 className="font-semibold">Active Connectors</h2>
        <QueryState isLoading={connectors.isLoading} error={connectors.error}>
          <ul className="space-y-2">
            {(connectors.data ?? []).length === 0 && (
              <li>
                <Card>
                  <CardContent className="p-3 text-sm text-muted-foreground">
                    No connectors yet.
                  </CardContent>
                </Card>
              </li>
            )}
            {(connectors.data ?? []).map((c) => (
              <li key={c.id}>
                <Card>
                  <CardContent className="flex flex-wrap items-center gap-3 p-3">
                    <span className="text-sm font-medium">{c.label}</span>
                    <Badge variant="secondary">{c.kind}</Badge>
                    <span className="text-xs text-muted-foreground">
                      Last synced: {c.last_synced_at ?? "never"}
                    </span>

                    {syncResults[c.id] && (
                      <span className="text-xs text-emerald-600 dark:text-emerald-400">
                        ✓ inserted {syncResults[c.id].inserted}, staged {syncResults[c.id].staged},
                        skipped {syncResults[c.id].skipped}
                      </span>
                    )}

                    <div className="ml-auto flex gap-2">
                      <Button
                        type="button"
                        size="sm"
                        aria-label={`Sync ${c.label}`}
                        disabled={syncConnector.isPending}
                        onClick={() => handleSync(c.id)}
                      >
                        Sync now
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        aria-label={`Delete ${c.label}`}
                        onClick={() => deleteConnector.mutate(c.id)}
                        className="text-destructive hover:text-destructive"
                      >
                        delete
                      </Button>
                    </div>
                  </CardContent>
                </Card>
              </li>
            ))}
          </ul>
        </QueryState>
      </section>
    </div>
  );
}
