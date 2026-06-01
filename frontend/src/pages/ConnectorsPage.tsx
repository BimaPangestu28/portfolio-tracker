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

const input = "rounded border px-2 py-1 text-sm";

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
    <div className="space-y-8">
      <h1 className="text-xl font-semibold">Connectors</h1>

      <section className="space-y-2">
        <h2 className="font-semibold">Add EVM Wallet Connector</h2>
        <form
          onSubmit={handleSubmit}
          className="grid grid-cols-1 gap-2 rounded border bg-white p-4 sm:grid-cols-2"
        >
          <select
            aria-label="Account"
            className={input}
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
          <input
            aria-label="Connector label"
            className={input}
            placeholder="Label (e.g. My ETH Wallet)"
            value={form.label}
            onChange={set("label")}
            required
          />
          <input
            aria-label="Wallet address"
            className={input}
            placeholder="0x… wallet address"
            value={form.address}
            onChange={set("address")}
            required
          />
          <input
            aria-label="Explorer base URL"
            className={input}
            placeholder="Explorer base URL (optional)"
            value={form.base_url}
            onChange={set("base_url")}
          />
          <input
            aria-label="API key"
            className={input}
            type="password"
            placeholder="API key (optional)"
            value={form.api_key}
            onChange={set("api_key")}
          />
          <div className="sm:col-span-2">
            <button
              type="submit"
              className="rounded bg-blue-600 px-3 py-1.5 text-sm text-white disabled:opacity-50"
              disabled={createConnector.isPending}
            >
              {createConnector.isPending ? "Adding…" : "Add connector"}
            </button>
            {createConnector.error && (
              <span className="ml-3 text-sm text-red-600">
                {(createConnector.error as Error).message}
              </span>
            )}
          </div>
        </form>
      </section>

      <section className="space-y-2">
        <h2 className="font-semibold">Connectors</h2>
        <QueryState isLoading={connectors.isLoading} error={connectors.error}>
          <ul className="space-y-2">
            {(connectors.data ?? []).length === 0 && (
              <li className="rounded border bg-white p-4 text-sm text-gray-500">
                No connectors yet. Add an EVM wallet above.
              </li>
            )}
            {(connectors.data ?? []).map((c) => (
              <li key={c.id} className="flex flex-wrap items-center gap-3 rounded border bg-white p-3 text-sm">
                <span className="font-medium">{c.label}</span>
                <span className="rounded bg-gray-100 px-1.5 py-0.5 text-xs">{c.kind}</span>
                <span className="text-gray-500">
                  {c.last_synced_at
                    ? `Last synced: ${c.last_synced_at.slice(0, 19).replace("T", " ")}`
                    : "Never synced"}
                </span>
                {syncResults[c.id] && (
                  <span className="text-xs text-green-700">
                    inserted: {syncResults[c.id].inserted} · staged: {syncResults[c.id].staged} · skipped: {syncResults[c.id].skipped}
                  </span>
                )}
                <div className="ml-auto flex gap-2">
                  <button
                    type="button"
                    aria-label={`Sync ${c.label}`}
                    className="rounded bg-green-600 px-2 py-0.5 text-xs text-white disabled:opacity-50"
                    onClick={() => handleSync(c.id)}
                    disabled={syncConnector.isPending}
                  >
                    Sync now
                  </button>
                  <button
                    type="button"
                    aria-label={`Delete ${c.label}`}
                    className="text-xs text-red-600 hover:underline"
                    onClick={() => deleteConnector.mutate(c.id)}
                  >
                    delete
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </QueryState>
      </section>
    </div>
  );
}
