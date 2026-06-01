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
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

const nativeSelect =
  "flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring";

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
        <Input
          aria-label="Month"
          type="month"
          className="w-40"
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
            <h2 className="text-sm font-semibold text-muted-foreground">By Category</h2>
            {(summaryData?.categories ?? []).map((cat) => {
              const actualNum = Number(cat.actual);
              const budgetNum = cat.budget != null ? Number(cat.budget) : null;
              const pct = budgetNum != null && budgetNum > 0 ? Math.min((actualNum / budgetNum) * 100, 100) : 0;
              return (
                <div key={cat.category_id ?? "uncategorized"} className="rounded-lg border bg-card p-3">
                  <div className="mb-1 flex items-center justify-between text-sm">
                    <span className={cat.over_budget ? "font-medium text-destructive" : "font-medium"}>{cat.name}</span>
                    <span className="text-muted-foreground">
                      {formatIDR(cat.actual)}
                      {cat.budget != null ? ` / ${formatIDR(cat.budget)}` : ""}
                      {cat.over_budget && <span className="ml-1 font-semibold text-destructive">over budget</span>}
                    </span>
                  </div>
                  {budgetNum != null && (
                    <div className="h-2 w-full rounded bg-muted">
                      <div
                        className={`h-2 rounded ${cat.over_budget ? "bg-destructive" : "bg-primary"}`}
                        style={{ width: `${pct}%` }}
                      />
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        ) : (
          <div className="text-sm text-muted-foreground">No category data for this month.</div>
        )}
      </QueryState>

      {/* Cashflow entry form */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Add Cashflow Entry</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={submitEntry} className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            <Input
              aria-label="Date"
              type="date"
              value={entryForm.occurred_on}
              onChange={setEntry("occurred_on")}
              required
            />
            <select
              aria-label="Direction"
              className={nativeSelect}
              value={entryForm.direction}
              onChange={setEntry("direction")}
            >
              <option value="in">in (income)</option>
              <option value="out">out (expense)</option>
            </select>
            <Input
              aria-label="Amount"
              placeholder="Amount"
              value={entryForm.amount}
              onChange={setEntry("amount")}
              required
            />
            <Input
              aria-label="Currency"
              placeholder="Currency"
              value={entryForm.currency}
              onChange={setEntry("currency")}
            />
            <select
              aria-label="Category"
              className={nativeSelect}
              value={entryForm.category_id}
              onChange={setEntry("category_id")}
            >
              <option value="">— no category —</option>
              {(categories.data ?? []).map((c) => (
                <option key={c.id} value={c.id}>{c.name}</option>
              ))}
            </select>
            <Input
              aria-label="Note"
              placeholder="Note (optional)"
              value={entryForm.note}
              onChange={setEntry("note")}
            />
            <Button
              type="submit"
              className="col-span-2 sm:col-span-3"
              disabled={createCashflow.isPending}
            >
              {createCashflow.isPending ? "Adding…" : "Add entry"}
            </Button>
            {createCashflow.error && (
              <div className="col-span-2 text-sm text-destructive sm:col-span-3">
                {(createCashflow.error as Error).message}
              </div>
            )}
          </form>
        </CardContent>
      </Card>

      {/* Category form */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Add Budget Category</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={submitCat} className="grid grid-cols-2 gap-2 sm:grid-cols-4">
            <Input
              aria-label="Category name"
              placeholder="Category name"
              value={catForm.name}
              onChange={setCat("name")}
              required
            />
            <select
              aria-label="Kind"
              className={nativeSelect}
              value={catForm.kind}
              onChange={setCat("kind")}
            >
              <option value="income">income</option>
              <option value="expense">expense</option>
            </select>
            <Input
              aria-label="Monthly budget"
              placeholder="Monthly budget (optional)"
              value={catForm.monthly_budget}
              onChange={setCat("monthly_budget")}
            />
            <Button
              type="submit"
              disabled={createCategory.isPending}
            >
              Add category
            </Button>
            {createCategory.error && (
              <div className="col-span-2 text-sm text-destructive sm:col-span-4">
                {(createCategory.error as Error).message}
              </div>
            )}
          </form>
        </CardContent>
      </Card>

      {/* Recent cashflow list */}
      <QueryState isLoading={cashflow.isLoading} error={cashflow.error}>
        <Card className="overflow-hidden">
          <CardHeader className="border-b px-4 py-2">
            <CardTitle className="text-sm">Recent Entries</CardTitle>
          </CardHeader>
          {(cashflow.data ?? []).length === 0 ? (
            <div className="p-4 text-sm text-muted-foreground">No cashflow entries yet.</div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Date</TableHead>
                  <TableHead>Direction</TableHead>
                  <TableHead>Amount</TableHead>
                  <TableHead>Currency</TableHead>
                  <TableHead>Note</TableHead>
                  <TableHead />
                </TableRow>
              </TableHeader>
              <TableBody>
                {(cashflow.data ?? []).map((cf) => (
                  <TableRow key={cf.id}>
                    <TableCell>{cf.occurred_on}</TableCell>
                    <TableCell>{cf.direction}</TableCell>
                    <TableCell>{cf.amount}</TableCell>
                    <TableCell>{cf.currency}</TableCell>
                    <TableCell>{cf.note ?? ""}</TableCell>
                    <TableCell className="text-right">
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => deleteCashflow.mutate(cf.id)}
                        className="text-destructive hover:text-destructive"
                      >
                        delete
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </Card>
      </QueryState>
    </div>
  );
}
