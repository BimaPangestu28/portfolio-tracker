import { useState } from "react";
import {
  useCashflow,
  useCashflowCategories,
  useMonthSummary,
  useCreateCashflow,
  useDeleteCashflow,
  useCreateCashflowCategory,
} from "../api/hooks";
import { StatCard } from "../components/StatCard";
import { QueryState } from "../components/QueryState";
import { formatIDR } from "../lib/format";

export default function BudgetPage() {
  const defaultMonth = new Date().toISOString().slice(0, 7);
  const [month, setMonth] = useState(defaultMonth);

  const summary = useMonthSummary(month);
  const cashflow = useCashflow();
  const categories = useCashflowCategories();
  const createCashflow = useCreateCashflow();
  const deleteCashflow = useDeleteCashflow();
  const createCategory = useCreateCashflowCategory();

  const [entryForm, setEntryForm] = useState({
    occurred_on: new Date().toISOString().slice(0, 10),
    direction: "out",
    amount: "",
    currency: "IDR",
    category_id: "",
    note: "",
  });

  const [catForm, setCatForm] = useState({
    name: "",
    kind: "expense",
    monthly_budget: "",
  });

  const setEntry = (k: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setEntryForm({ ...entryForm, [k]: e.target.value });

  const setCat = (k: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setCatForm({ ...catForm, [k]: e.target.value });

  const input = "rounded border px-2 py-1 text-sm";

  const submitEntry = (e: React.FormEvent) => {
    e.preventDefault();
    createCashflow.mutate({
      occurred_on: entryForm.occurred_on,
      direction: entryForm.direction,
      amount: entryForm.amount,
      currency: entryForm.currency,
      category_id: entryForm.category_id ? Number(entryForm.category_id) : null,
      note: entryForm.note || null,
      account_id: null,
    });
    setEntryForm({
      occurred_on: new Date().toISOString().slice(0, 10),
      direction: "out",
      amount: "",
      currency: "IDR",
      category_id: "",
      note: "",
    });
  };

  const submitCat = (e: React.FormEvent) => {
    e.preventDefault();
    createCategory.mutate({
      name: catForm.name,
      kind: catForm.kind,
      monthly_budget: catForm.monthly_budget || null,
      color: null,
    });
    setCatForm({ name: "", kind: "expense", monthly_budget: "" });
  };

  const summaryData = summary.data;

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4">
        <h1 className="text-xl font-semibold">Budget</h1>
        <input
          aria-label="Month"
          type="month"
          className={input}
          value={month}
          onChange={(e) => setMonth(e.target.value)}
        />
      </div>

      <QueryState isLoading={summary.isLoading} error={summary.error}>
        <div className="grid grid-cols-3 gap-4">
          <StatCard label="Income" value={formatIDR(summaryData?.total_in ?? "0")} tone="pos" />
          <StatCard label="Expense" value={formatIDR(summaryData?.total_out ?? "0")} tone="neg" />
          <StatCard
            label="Net"
            value={formatIDR(summaryData?.net ?? "0")}
            tone={Number(summaryData?.net ?? 0) >= 0 ? "pos" : "neg"}
          />
        </div>

        {(summaryData?.categories ?? []).length > 0 ? (
          <div className="space-y-2">
            <h2 className="text-sm font-semibold text-gray-700">By Category</h2>
            {(summaryData?.categories ?? []).map((cat) => {
              const actualNum = Number(cat.actual);
              const budgetNum = cat.budget != null ? Number(cat.budget) : null;
              const pct = budgetNum != null && budgetNum > 0 ? Math.min((actualNum / budgetNum) * 100, 100) : 0;
              return (
                <div key={cat.category_id ?? "uncategorized"} className="rounded border bg-white p-3">
                  <div className="mb-1 flex items-center justify-between text-sm">
                    <span className={cat.over_budget ? "font-medium text-red-600" : "font-medium"}>{cat.name}</span>
                    <span className="text-gray-500">
                      {formatIDR(cat.actual)}
                      {cat.budget != null ? ` / ${formatIDR(cat.budget)}` : ""}
                      {cat.over_budget && <span className="ml-1 text-red-600 font-semibold">over budget</span>}
                    </span>
                  </div>
                  {budgetNum != null && (
                    <div className="h-2 w-full rounded bg-gray-100">
                      <div
                        className={`h-2 rounded ${cat.over_budget ? "bg-red-500" : "bg-blue-500"}`}
                        style={{ width: `${pct}%` }}
                      />
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        ) : (
          <div className="text-sm text-gray-500">No category data for this month.</div>
        )}
      </QueryState>

      {/* Cashflow entry form */}
      <div className="rounded border bg-white p-4">
        <h2 className="mb-3 text-sm font-semibold">Add Cashflow Entry</h2>
        <form onSubmit={submitEntry} className="grid grid-cols-2 gap-2 sm:grid-cols-3">
          <input
            aria-label="Date"
            type="date"
            className={input}
            value={entryForm.occurred_on}
            onChange={setEntry("occurred_on")}
            required
          />
          <select
            aria-label="Direction"
            className={input}
            value={entryForm.direction}
            onChange={setEntry("direction")}
          >
            <option value="in">in (income)</option>
            <option value="out">out (expense)</option>
          </select>
          <input
            aria-label="Amount"
            className={input}
            placeholder="Amount"
            value={entryForm.amount}
            onChange={setEntry("amount")}
            required
          />
          <input
            aria-label="Currency"
            className={input}
            placeholder="Currency"
            value={entryForm.currency}
            onChange={setEntry("currency")}
          />
          <select
            aria-label="Category"
            className={input}
            value={entryForm.category_id}
            onChange={setEntry("category_id")}
          >
            <option value="">— no category —</option>
            {(categories.data ?? []).map((c) => (
              <option key={c.id} value={c.id}>{c.name}</option>
            ))}
          </select>
          <input
            aria-label="Note"
            className={input}
            placeholder="Note (optional)"
            value={entryForm.note}
            onChange={setEntry("note")}
          />
          <button
            type="submit"
            className="col-span-2 rounded bg-blue-600 px-3 py-1.5 text-sm text-white disabled:opacity-50 sm:col-span-3"
            disabled={createCashflow.isPending}
          >
            {createCashflow.isPending ? "Adding…" : "Add entry"}
          </button>
          {createCashflow.error && (
            <div className="col-span-2 text-sm text-red-600 sm:col-span-3">
              {(createCashflow.error as Error).message}
            </div>
          )}
        </form>
      </div>

      {/* Category form */}
      <div className="rounded border bg-white p-4">
        <h2 className="mb-3 text-sm font-semibold">Add Budget Category</h2>
        <form onSubmit={submitCat} className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <input
            aria-label="Category name"
            className={input}
            placeholder="Category name"
            value={catForm.name}
            onChange={setCat("name")}
            required
          />
          <select
            aria-label="Kind"
            className={input}
            value={catForm.kind}
            onChange={setCat("kind")}
          >
            <option value="income">income</option>
            <option value="expense">expense</option>
          </select>
          <input
            aria-label="Monthly budget"
            className={input}
            placeholder="Monthly budget (optional)"
            value={catForm.monthly_budget}
            onChange={setCat("monthly_budget")}
          />
          <button
            type="submit"
            className="rounded bg-blue-600 px-3 py-1.5 text-sm text-white disabled:opacity-50"
            disabled={createCategory.isPending}
          >
            Add category
          </button>
          {createCategory.error && (
            <div className="col-span-2 text-sm text-red-600 sm:col-span-4">
              {(createCategory.error as Error).message}
            </div>
          )}
        </form>
      </div>

      {/* Recent cashflow list */}
      <QueryState isLoading={cashflow.isLoading} error={cashflow.error}>
        <div className="rounded border bg-white">
          <h2 className="border-b px-4 py-2 text-sm font-semibold">Recent Entries</h2>
          {(cashflow.data ?? []).length === 0 ? (
            <div className="p-4 text-sm text-gray-500">No cashflow entries yet.</div>
          ) : (
            <table className="w-full text-sm">
              <thead className="bg-gray-50 text-left text-xs uppercase text-gray-500">
                <tr>
                  <th className="p-2">Date</th>
                  <th className="p-2">Direction</th>
                  <th className="p-2">Amount</th>
                  <th className="p-2">Currency</th>
                  <th className="p-2">Note</th>
                  <th className="p-2"></th>
                </tr>
              </thead>
              <tbody>
                {(cashflow.data ?? []).map((cf) => (
                  <tr key={cf.id} className="border-t">
                    <td className="p-2">{cf.occurred_on}</td>
                    <td className="p-2">{cf.direction}</td>
                    <td className="p-2">{cf.amount}</td>
                    <td className="p-2">{cf.currency}</td>
                    <td className="p-2">{cf.note ?? ""}</td>
                    <td className="p-2 text-right">
                      <button
                        type="button"
                        onClick={() => deleteCashflow.mutate(cf.id)}
                        className="text-xs text-red-600 hover:underline"
                      >
                        delete
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </QueryState>
    </div>
  );
}
