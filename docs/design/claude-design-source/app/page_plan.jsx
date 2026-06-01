/* ============================================================
   Planner (allocation targets) + Budget (cashflow)
   ============================================================ */
const { Card: Card, Icon: Icon, Badge: Badge, Button: Button, Dialog: Dialog, Field: Field, Input: Input, Select: Select, Progress: Progress, Empty: Empty } = window;
const { useState: pState } = React;

/* ---------------- Planner ---------------- */
function Planner({ loading }) {
  const P = window.PT;
  const [targets, setTargets] = pState(P.DRIFT);
  const [open, setOpen] = pState(false);
  const [form, setForm] = pState({ cat: '', target: '', tol: '5' });
  const sum = targets.reduce((s, t) => s + t.target, 0);
  const sumOk = sum === 100;

  const add = () => {
    if (!form.cat || !form.target) { window.toast('Lengkapi kategori dan target', 'warn'); return; }
    setTargets(t => [...t, { id: 'x' + Date.now(), label: form.cat, target: +form.target, tol: +form.tol, actual: 0, deltaValue: (+form.target / 100) * P.NET_WORTH, outOfBand: true }]);
    setOpen(false); setForm({ cat: '', target: '', tol: '5' });
    window.toast('Kategori target ditambahkan', 'success');
  };

  return (
    <div>
      <PageHeader title="Planner" sub="Target alokasi & batas toleransi"
        actions={<Button variant="primary" icon="plus" size="sm" onClick={() => setOpen(true)}>Tambah Kategori</Button>} />

      <Card className="card-pad" style={{ marginBottom: 18, display: 'flex', alignItems: 'center', gap: 16, flexWrap: 'wrap' }}>
        <div className="flex items-center gap-3 flex-1">
          <span className={'flex items-center justify-center'} style={{ width: 40, height: 40, borderRadius: 11, background: sumOk ? 'hsl(var(--gain-soft))' : 'hsl(var(--warn-soft))', color: sumOk ? 'hsl(var(--gain))' : 'hsl(var(--warn))' }}>
            <Icon name={sumOk ? 'checkCircle' : 'alertTriangle'} size={20} />
          </span>
          <div>
            <div className="t-h3">Total target alokasi {sum}%</div>
            <div className="t-sm t-muted">{sumOk ? 'Seimbang — target berjumlah tepat 100%.' : `Perlu disesuaikan ${100 - sum > 0 ? '+' : ''}${100 - sum}% agar mencapai 100%.`}</div>
          </div>
        </div>
        <div style={{ width: 200 }}><Progress value={Math.min(sum, 100)} color={sumOk ? 'hsl(var(--gain))' : 'hsl(var(--warn))'} /></div>
      </Card>

      <div className="grid gap-4" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))' }}>
        {targets.map(t => (
          <Card key={t.id} className="card-pad">
            <div className="flex items-center justify-between" style={{ marginBottom: 14 }}>
              <div className="flex items-center gap-2">
                <span className="dot" style={{ background: P.catColor(t.id), width: 11, height: 11 }}></span>
                <span style={{ fontWeight: 600 }}>{t.label}</span>
              </div>
              {t.outOfBand ? <Badge tone="warn">drift {t.drift > 0 ? '+' : ''}{(t.actual - t.target).toFixed(1).replace('.', ',')}%</Badge> : <Badge tone="gain">on target</Badge>}
            </div>
            <div className="flex items-end justify-between" style={{ marginBottom: 10 }}>
              <div><div className="t-xs t-muted">Aktual</div><div className="t-h2 num">{t.actual.toFixed(1).replace('.', ',')}%</div></div>
              <div className="text-right"><div className="t-xs t-muted">Target ±{t.tol}%</div><div className="t-h2 num t-muted">{t.target}%</div></div>
            </div>
            <Progress value={(t.actual / Math.max(t.target * 1.6, t.actual)) * 100} color={P.catColor(t.id)} />
            {t.outOfBand && <div className="t-xs warn num" style={{ marginTop: 8, fontWeight: 540 }}>{t.deltaValue > 0 ? 'Beli ' : 'Pangkas '}{P.formatIDR(Math.abs(t.deltaValue), { compact: true })}</div>}
          </Card>
        ))}
      </div>

      <Dialog open={open} onClose={() => setOpen(false)} title="Tambah Kategori Target" sub="Tetapkan alokasi target & batas toleransi"
        footer={<><Button onClick={() => setOpen(false)}>Batal</Button><Button variant="primary" icon="check" onClick={add}>Simpan</Button></>}>
        <Field label="Nama Kategori"><Input placeholder="mis. Saham US, Properti…" value={form.cat} onChange={e => setForm(f => ({ ...f, cat: e.target.value }))} /></Field>
        <div className="grid gap-3" style={{ gridTemplateColumns: '1fr 1fr' }}>
          <Field label="Target %"><Input type="number" placeholder="0" value={form.target} onChange={e => setForm(f => ({ ...f, target: e.target.value }))} /></Field>
          <Field label="Toleransi ± %"><Input type="number" placeholder="5" value={form.tol} onChange={e => setForm(f => ({ ...f, tol: e.target.value }))} /></Field>
        </div>
      </Dialog>
    </div>
  );
}
window.Planner = Planner;

/* ---------------- Budget ---------------- */
const MONTHS_ID = { '2026-03': 'Maret 2026', '2026-04': 'April 2026', '2026-05': 'Mei 2026' };

function Budget({ loading }) {
  const P = window.PT;
  const [month, setMonth] = pState('2026-05');
  const [cats] = pState(P.BUDGET_CATS);
  const [flows, setFlows] = pState(P.CASHFLOW);
  const [open, setOpen] = pState(false);
  const [form, setForm] = pState({ kind: 'out', label: '', cat: P.BUDGET_CATS[0].label, amount: '', date: '2026-05-31' });
  const k = P.BUDGET_KPIS;

  const add = () => {
    if (!form.label || !form.amount) { window.toast('Lengkapi keterangan dan nominal', 'warn'); return; }
    setFlows(f => [{ id: 'f' + Date.now(), date: form.date, label: form.label, cat: form.kind === 'in' ? 'Pendapatan' : form.cat, amount: +form.amount, kind: form.kind }, ...f]);
    setOpen(false); setForm({ kind: 'out', label: '', cat: P.BUDGET_CATS[0].label, amount: '', date: '2026-05-31' });
    window.toast('Arus kas dicatat', 'success');
  };

  const kpi = (label, val, icon, tone) => (
    <Card className="stat-card">
      <div className="stat-label"><Icon name={icon} size={15} />{label}</div>
      <div className={'stat-value num ' + (tone || '')}>{P.formatIDR(val, { compact: true })}</div>
      <div className="t-xs t-muted">{P.formatUSD(P.idrToUsd(val))}</div>
    </Card>
  );

  return (
    <div>
      <PageHeader title="Budget" sub="Arus kas & anggaran bulanan"
        actions={<>
          <Select value={month} onChange={e => setMonth(e.target.value)} style={{ width: 'auto', minWidth: 150 }}>{Object.keys(MONTHS_ID).map(m => <option key={m} value={m}>{MONTHS_ID[m]}</option>)}</Select>
          <Button variant="primary" icon="plus" size="sm" onClick={() => setOpen(true)}>Catat</Button>
        </>} />

      <div className="grid gap-4" style={{ gridTemplateColumns: 'repeat(3,1fr)', marginBottom: 18 }}>
        {kpi('Pemasukan', k.income, 'arrowDown', 'gain')}
        {kpi('Pengeluaran', k.expense, 'arrowUp', 'loss')}
        {kpi('Arus Kas Bersih', k.net, 'scale', 'gain')}
      </div>

      <div className="grid gap-4" style={{ gridTemplateColumns: 'minmax(0,1.5fr) minmax(0,1fr)' }}>
        <Card>
          <div className="card-head"><div><div className="card-title">Anggaran per Kategori</div><div className="card-sub">terpakai vs anggaran bulan ini</div></div></div>
          <div className="card-pad flex col gap-4" style={{ paddingTop: 16 }}>
            {cats.map(c => {
              const ratio = (c.spent / c.budget) * 100;
              const over = c.spent > c.budget;
              const near = !over && ratio > 85;
              const color = over ? 'hsl(var(--loss))' : near ? 'hsl(var(--warn))' : 'hsl(var(--primary))';
              return (
                <div key={c.id} className="flex col gap-2">
                  <div className="flex items-center justify-between">
                    <span className="t-sm" style={{ fontWeight: 540 }}>{c.label}</span>
                    <span className="t-sm num">
                      <span style={{ color, fontWeight: 580 }}>{P.formatIDR(c.spent, { compact: true })}</span>
                      <span className="t-muted"> / {P.formatIDR(c.budget, { compact: true })}</span>
                    </span>
                  </div>
                  <Progress value={ratio} color={color} />
                  {over && <span className="t-xs loss num">Lebih {P.formatIDR(c.spent - c.budget, { compact: true })} dari anggaran</span>}
                </div>
              );
            })}
          </div>
        </Card>

        <Card>
          <div className="card-head"><div><div className="card-title">Arus Kas Terbaru</div></div></div>
          <div className="flex col" style={{ padding: '8px 0' }}>
            {flows.map(f => (
              <div key={f.id} className="flex items-center gap-3" style={{ padding: '11px 20px' }}>
                <span style={{ width: 34, height: 34, borderRadius: 9, display: 'grid', placeItems: 'center', flexShrink: 0, background: f.kind === 'in' ? 'hsl(var(--gain-soft))' : 'hsl(var(--muted))', color: f.kind === 'in' ? 'hsl(var(--gain))' : 'hsl(var(--muted-foreground))' }}>
                  <Icon name={f.kind === 'in' ? 'arrowDown' : 'arrowUp'} size={16} sw={2.4} />
                </span>
                <div className="flex-1" style={{ minWidth: 0 }}>
                  <div className="t-sm truncate" style={{ fontWeight: 540 }}>{f.label}</div>
                  <div className="t-xs t-muted truncate">{f.cat} · {f.date}</div>
                </div>
                <span className={'num t-sm ' + (f.kind === 'in' ? 'gain' : '')} style={{ fontWeight: 580 }}>{f.kind === 'in' ? '+' : '−'}{P.formatIDR(f.amount, { compact: true })}</span>
              </div>
            ))}
          </div>
        </Card>
      </div>

      <Dialog open={open} onClose={() => setOpen(false)} title="Catat Arus Kas" sub="Tambahkan pemasukan atau pengeluaran"
        footer={<><Button onClick={() => setOpen(false)}>Batal</Button><Button variant="primary" icon="check" onClick={add}>Simpan</Button></>}>
        <div className="seg" style={{ alignSelf: 'flex-start' }}>
          <button className={form.kind === 'out' ? 'active' : ''} onClick={() => setForm(f => ({ ...f, kind: 'out' }))}>Pengeluaran</button>
          <button className={form.kind === 'in' ? 'active' : ''} onClick={() => setForm(f => ({ ...f, kind: 'in' }))}>Pemasukan</button>
        </div>
        <Field label="Keterangan"><Input placeholder="mis. Belanja bulanan" value={form.label} onChange={e => setForm(f => ({ ...f, label: e.target.value }))} /></Field>
        <div className="grid gap-3" style={{ gridTemplateColumns: '1fr 1fr' }}>
          <Field label="Nominal (IDR)"><Input type="number" placeholder="0" value={form.amount} onChange={e => setForm(f => ({ ...f, amount: e.target.value }))} /></Field>
          <Field label="Tanggal"><Input type="date" value={form.date} onChange={e => setForm(f => ({ ...f, date: e.target.value }))} /></Field>
        </div>
        {form.kind === 'out' && <Field label="Kategori"><Select value={form.cat} onChange={e => setForm(f => ({ ...f, cat: e.target.value }))}>{P.BUDGET_CATS.map(c => <option key={c.id}>{c.label}</option>)}</Select></Field>}
      </Dialog>
    </div>
  );
}
window.Budget = Budget;
