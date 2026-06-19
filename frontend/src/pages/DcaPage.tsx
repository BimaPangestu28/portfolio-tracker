import { useState, useEffect } from "react";
import { Repeat, Save } from "lucide-react";
import { toast } from "sonner";
import { useDcaSettings, useUpdateDcaSettings, useDcaPlan } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { formatIDR, formatPct, parseNum } from "../lib/format";

const MODE_LABEL: Record<string, string> = {
  rebalance: "Rebalancing",
  mixed: "Rebalancing + Proporsional",
  proportional: "Proporsional",
  empty: "Belum ada target",
};

export default function DcaPage() {
  const settings = useDcaSettings();
  const plan = useDcaPlan();
  const save = useUpdateDcaSettings();

  const [form, setForm] = useState({
    monthly_budget: "",
    frequency: "monthly",
    anchor_day: "1",
    rounding_step: "10000",
  });

  // Seed the form once settings load.
  useEffect(() => {
    if (settings.data) {
      setForm({
        monthly_budget: settings.data.monthly_budget,
        frequency: settings.data.frequency,
        anchor_day: String(settings.data.anchor_day),
        rounding_step: settings.data.rounding_step,
      });
    }
  }, [settings.data]);

  const set = (k: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setForm((prev) => ({ ...prev, [k]: e.target.value }));

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    save.mutate(
      {
        monthly_budget: form.monthly_budget || "0",
        frequency: form.frequency as "monthly" | "weekly",
        anchor_day: Number(form.anchor_day),
        rounding_step: form.rounding_step || "10000",
      },
      {
        onSuccess: () => toast.success("Setelan DCA disimpan"),
        onError: (err) => toast.error((err as Error).message),
      },
    );
  };

  return (
    <div>
      <div className="flex items-center justify-between" style={{ marginBottom: 18, flexWrap: "wrap", gap: 10 }}>
        <div>
          <h1 className="t-h1">DCA Planner</h1>
          <div className="t-sm t-muted" style={{ marginTop: 2 }}>
            Alokasi kontribusi rutin menuju target diversifikasi
          </div>
        </div>
      </div>

      <div className="lay-2-15col" style={{ gap: 16 }}>
        {/* Settings */}
        <div className="card">
          <div className="card-head">
            <div>
              <div className="card-title">Setelan</div>
              <div className="card-sub">budget &amp; frekuensi</div>
            </div>
          </div>
          <form className="card-pad" style={{ paddingTop: 16 }} onSubmit={submit}>
            <label className="field">
              <span className="field-label">Budget bulanan (IDR)</span>
              <input type="number" className="input" placeholder="55000000"
                     value={form.monthly_budget} onChange={set("monthly_budget")} />
            </label>
            <div className="grid form-stack" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
              <label className="field">
                <span className="field-label">Frekuensi</span>
                <select className="select" value={form.frequency} onChange={set("frequency")}>
                  <option value="monthly">Bulanan</option>
                  <option value="weekly">Mingguan</option>
                </select>
              </label>
              <label className="field">
                <span className="field-label">Tanggal anchor</span>
                <input type="number" min={1} max={28} className="input"
                       value={form.anchor_day} onChange={set("anchor_day")} />
              </label>
            </div>
            <label className="field">
              <span className="field-label">Pembulatan (IDR)</span>
              <input type="number" className="input" placeholder="10000"
                     value={form.rounding_step} onChange={set("rounding_step")} />
            </label>
            <button type="submit" className="btn btn-primary" disabled={save.isPending}
                    style={{ marginTop: 8 }}>
              <Save size={16} /> Simpan
            </button>
          </form>
        </div>

        {/* Plan */}
        <div className="card">
          <div className="card-head">
            <div>
              <div className="card-title">Rencana periode ini</div>
              <div className="card-sub">
                <Repeat size={13} style={{ display: "inline", verticalAlign: "-2px" }} />{" "}
                {plan.data ? `${MODE_LABEL[plan.data.mode] ?? plan.data.mode} · budget ${formatIDR(plan.data.budget_idr)}` : "—"}
              </div>
            </div>
          </div>
          <div className="card-pad" style={{ paddingTop: 16 }}>
            <QueryState isLoading={plan.isLoading} error={plan.error}>
              {plan.data && plan.data.note && (
                <div className="t-sm t-muted" style={{ marginBottom: 12 }}>{plan.data.note}</div>
              )}
              <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
                {(plan.data?.lines ?? []).map((l) => {
                  const alloc = parseNum(l.allocate_idr);
                  const budget = parseNum(plan.data?.budget_idr ?? "0");
                  const ratio = budget > 0 ? Math.min((alloc / budget) * 100, 100) : 0;
                  const muted = alloc <= 0;
                  return (
                    <div key={l.category_id} style={{ display: "flex", flexDirection: "column", gap: 6, opacity: muted ? 0.55 : 1 }}>
                      <div className="flex items-center justify-between">
                        <span className="t-sm" style={{ fontWeight: 500 }}>
                          {l.name}
                          <span className="t-muted" style={{ fontWeight: 400 }}>
                            {" "}· {formatPct(l.actual_pct)} / target {formatPct(l.target_pct)}
                          </span>
                        </span>
                        <span className="t-sm num" style={{ fontWeight: 600 }}>{formatIDR(l.allocate_idr)}</span>
                      </div>
                      <div className="progress">
                        <span style={{ width: `${ratio}%`, background: muted ? "hsl(var(--muted-foreground))" : "hsl(var(--primary))" }} />
                      </div>
                    </div>
                  );
                })}
              </div>
            </QueryState>
          </div>
        </div>
      </div>
    </div>
  );
}
