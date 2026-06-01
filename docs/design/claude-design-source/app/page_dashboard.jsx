/* ============================================================
   Dashboard — financial-planner command center
   Health · Allocation · Actions · Composition · Movers · Goals
   ============================================================ */
const { Card: Card, Icon: Icon, Badge: Badge, Skeleton: Skeleton, Donut: Donut, AreaChart: AreaChart, StackedArea: StackedArea, DriftBar: DriftBar, Seg: Seg, Button: Button, Empty: Empty, Progress: Progress } = window;

function StatCard({ label, icon, value, sub, tone, loading }) {
  if (loading) return <div className="card stat-card"><Skeleton w={90} h={12} /><Skeleton w={140} h={24} /><Skeleton w={70} h={12} /></div>;
  return (
    <div className="card stat-card">
      <div className="stat-label"><Icon name={icon} size={15} />{label}</div>
      <div className={'stat-value num ' + (tone || '')}>{value}</div>
      {sub}
    </div>
  );
}

const STT = {
  good: { color: 'hsl(var(--gain))', soft: 'hsl(var(--gain-soft))', label: 'Sehat' },
  warn: { color: 'hsl(var(--warn))', soft: 'hsl(var(--warn-soft))', label: 'Perhatikan' },
  bad: { color: 'hsl(var(--loss))', soft: 'hsl(var(--loss-soft))', label: 'Risiko' },
};
function MetricRow({ icon, label, value, hint, status }) {
  const s = STT[status];
  return (
    <div className="flex items-center gap-3" style={{ padding: '12px 0', borderBottom: '1px solid hsl(var(--border))' }}>
      <span style={{ width: 36, height: 36, borderRadius: 10, flexShrink: 0, display: 'grid', placeItems: 'center', color: s.color, background: s.soft }}><Icon name={icon} size={17} /></span>
      <div className="flex-1" style={{ minWidth: 0 }}>
        <div className="flex items-center justify-between gap-2">
          <span className="t-sm" style={{ fontWeight: 540 }}>{label}</span>
          <span className="num t-sm" style={{ fontWeight: 620, color: s.color }}>{value}</span>
        </div>
        {hint && <div className="t-xs t-muted">{hint}</div>}
      </div>
    </div>
  );
}

function Dashboard({ loading, base = 'IDR' }) {
  const P = window.PT;
  const R = window.React;
  const [range, setRange] = R.useState('1y');
  const usd = base === 'USD';
  const money = (v, compact) => usd ? P.formatUSD(P.idrToUsd(v)) : P.formatIDR(v, { compact });
  const allocData = P.ALLOC.map(a => ({ id: a.id, label: a.label, value: a.value, color: P.catColor(a.id) }));
  const driftScale = Math.max(...P.DRIFT.map(d => Math.max(d.actual, d.target + d.tol))) * 1.05;
  const uplPct = (P.UNREALIZED / P.TOTAL_COST) * 100;
  const actions = P.DRIFT.filter(d => d.outOfBand);

  // hero sparkline
  const hv = P.HISTORY.map(d => d.value);
  const hmin = Math.min(...hv), hmax = Math.max(...hv);
  const sLine = hv.map((v, i) => `${i ? 'L' : 'M'} ${((i / (hv.length - 1)) * 100).toFixed(2)} ${(28 - ((v - hmin) / (hmax - hmin || 1)) * 26).toFixed(2)}`).join(' ');
  const sArea = sLine + ' L 100 30 L 0 30 Z';
  const twelveMoPct = ((hv[hv.length - 1] - hv[0]) / hv[0]) * 100;

  const stackSlice = { '3m': P.STACK.slice(-3), '6m': P.STACK.slice(-6), '1y': P.STACK }[range];
  const go = (k) => { location.hash = '/' + k; };

  const runwayStatus = P.RUNWAY_MONTHS >= 6 ? 'good' : P.RUNWAY_MONTHS >= 3 ? 'warn' : 'bad';
  const concStatus = P.TOP_HOLDING_PCT < 20 ? 'good' : P.TOP_HOLDING_PCT < 30 ? 'warn' : 'bad';
  const saveStatus = P.SAVINGS_RATE >= 30 ? 'good' : P.SAVINGS_RATE >= 15 ? 'warn' : 'bad';
  const driftStatus = actions.length === 0 ? 'good' : actions.length <= 2 ? 'warn' : 'bad';

  return (
    <div className="flex col gap-5">
      {/* ===== hero + KPIs ===== */}
      <div className="grid gap-5" style={{ gridTemplateColumns: 'minmax(0,1.15fr) minmax(0,1fr)' }}>
        <div className="card card-pad" style={{ display: 'flex', flexDirection: 'column', justifyContent: 'space-between', gap: 16, minHeight: 200 }}>
          <div className="flex items-center justify-between">
            <span className="t-label" style={{ whiteSpace: 'nowrap' }}>Total Kekayaan Bersih</span>
            <Badge tone="gain" dot>Live</Badge>
          </div>
          {loading ? <Skeleton w={320} h={44} /> : (
            <div className="flex col gap-2">
              <div className="flex items-end gap-3 flex-wrap">
                <span className="t-display num">{usd ? P.formatUSD(P.idrToUsd(P.NET_WORTH)) : P.formatIDR(P.NET_WORTH)}</span>
                <span className="num t-muted" style={{ fontSize: 17, fontWeight: 540, paddingBottom: 5 }}>≈ {usd ? P.formatIDR(P.NET_WORTH) : P.formatUSD(P.idrToUsd(P.NET_WORTH))}</span>
              </div>
              <div className="flex items-center gap-3 flex-wrap">
                <span className="stat-delta num gain"><Icon name="arrowUp" size={15} sw={2.4} />{P.formatIDR(P.DAY_DELTA, { compact: true })} ({P.formatPct(P.DAY_DELTA_PCT)})</span>
                <span className="t-xs t-muted">hari ini · 12 bln {P.formatPct(twelveMoPct, { digits: 1 })}</span>
              </div>
            </div>
          )}
          {!loading && (
            <svg viewBox="0 0 100 30" preserveAspectRatio="none" style={{ width: '100%', height: 46, display: 'block', overflow: 'visible' }}>
              <defs><linearGradient id="heroSpark" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stopColor="hsl(var(--gain))" stopOpacity="0.22" /><stop offset="100%" stopColor="hsl(var(--gain))" stopOpacity="0" /></linearGradient></defs>
              <path d={sArea} fill="url(#heroSpark)" />
              <path d={sLine} fill="none" stroke="hsl(var(--gain))" strokeWidth="1.6" vectorEffect="non-scaling-stroke" strokeLinejoin="round" strokeLinecap="round" />
            </svg>
          )}
        </div>
        <div className="grid gap-5" style={{ gridTemplateColumns: '1fr 1fr' }}>
          <StatCard loading={loading} label="Unrealized P&L" icon="trendUp" value={money(P.UNREALIZED, true)} tone="gain"
            sub={<span className="stat-delta gain num"><Icon name="arrowUp" size={13} sw={2.4} />{P.formatPct(uplPct)}</span>} />
          <StatCard loading={loading} label="XIRR" icon="scale" value={P.formatPct(P.XIRR, { digits: 1 })} tone="gain"
            sub={<span className="t-xs t-muted">tahunan, sejak awal</span>} />
          <StatCard loading={loading} label="Imbal Hasil Pasif" icon="coins" value={P.YIELD_PCT.toFixed(2).replace('.', ',') + '%'}
            sub={<span className="t-xs t-muted">{money(P.DIVIDEND_TTM, true)} dividen 12 bln</span>} />
          <StatCard loading={loading} label="Tingkat Menabung" icon="banknote" value={P.SAVINGS_RATE.toFixed(0) + '%'} tone="gain"
            sub={<span className="t-xs t-muted">{money(P.BUDGET_KPIS.net, true)}/bln disisihkan</span>} />
        </div>
      </div>

      {/* ===== allocation + drift ===== */}
      <div className="grid gap-5" style={{ gridTemplateColumns: 'minmax(0,1fr) minmax(0,1.25fr)' }}>
        <div className="card">
          <div className="card-head"><div><div className="card-title">Alokasi Aset</div><div className="card-sub">berdasarkan nilai pasar</div></div></div>
          <div className="card-pad flex items-center gap-5" style={{ paddingTop: 14 }}>
            {loading ? <Skeleton w={180} h={180} r={999} /> : (
              <Donut data={allocData} center={<div><div className="t-xs t-muted">Total</div><div className="t-h3 num">{P.formatIDR(P.NET_WORTH, { compact: true })}</div></div>} />
            )}
            <div className="flex col gap-2 flex-1">
              {(loading ? P.ALLOC.slice(0, 4) : P.ALLOC).map(a => (
                <div key={a.id} className="flex items-center gap-2">
                  <span className="dot" style={{ background: P.catColor(a.id) }}></span>
                  {loading ? <Skeleton w={120} h={12} /> : <>
                    <span className="t-sm flex-1 truncate">{a.label}</span>
                    <span className="t-sm num" style={{ fontWeight: 580 }}>{a.pct.toFixed(1).replace('.', ',')}%</span>
                  </>}
                </div>
              ))}
            </div>
          </div>
        </div>
        <div className="card">
          <div className="card-head">
            <div><div className="card-title">Target vs Aktual</div><div className="card-sub">drift dari alokasi target</div></div>
            <Badge tone={actions.length ? 'warn' : 'gain'}>{actions.length} di luar batas</Badge>
          </div>
          <div className="card-pad flex col gap-3" style={{ paddingTop: 14 }}>
            {P.DRIFT.map(d => (
              <div key={d.id} className="flex col gap-1">
                <div className="flex items-center justify-between">
                  <span className="t-sm flex items-center gap-2"><span className="dot" style={{ background: P.catColor(d.id) }}></span>{d.label}</span>
                  <span className="t-xs num t-muted">{d.actual.toFixed(1).replace('.', ',')}% / {d.target}%</span>
                </div>
                <DriftBar row={d} scaleMax={driftScale} />
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* ===== rebalancing actions + health ===== */}
      <div className="grid gap-5" style={{ gridTemplateColumns: 'minmax(0,1fr) minmax(0,1fr)' }}>
        <div className="card">
          <div className="card-head">
            <div><div className="card-title">Rekomendasi Rebalancing</div><div className="card-sub">langkah agar kembali ke target</div></div>
            <Button size="sm" variant="ghost" iconRight="arrowRight" onClick={() => go('planner')}>Planner</Button>
          </div>
          <div className="flex col" style={{ padding: '6px 20px 18px' }}>
            {actions.length === 0 ? (
              <div className="flex items-center gap-3" style={{ padding: '18px 0' }}><span style={{ width: 36, height: 36, borderRadius: 10, background: 'hsl(var(--gain-soft))', color: 'hsl(var(--gain))', display: 'grid', placeItems: 'center' }}><Icon name="checkCircle" size={18} /></span><div><div className="t-sm" style={{ fontWeight: 600 }}>Portofolio seimbang</div><div className="t-xs t-muted">Semua kategori dalam batas toleransi.</div></div></div>
            ) : actions.map(d => {
              const buy = d.deltaValue > 0;
              return (
                <div key={d.id} className="flex items-center gap-3" style={{ padding: '13px 0', borderBottom: '1px solid hsl(var(--border))' }}>
                  <span style={{ width: 36, height: 36, borderRadius: 10, flexShrink: 0, display: 'grid', placeItems: 'center', background: buy ? 'hsl(var(--gain-soft))' : 'hsl(var(--warn-soft))', color: buy ? 'hsl(var(--gain))' : 'hsl(var(--warn))' }}><Icon name={buy ? 'arrowDown' : 'arrowUp'} size={17} sw={2.4} /></span>
                  <div className="flex-1" style={{ minWidth: 0 }}>
                    <div className="t-sm" style={{ fontWeight: 600 }}>{buy ? 'Beli' : 'Pangkas'} {d.label}</div>
                    <div className="t-xs t-muted">{d.actual.toFixed(1).replace('.', ',')}% → target {d.target}% · selisih {(d.actual - d.target).toFixed(1).replace('.', ',')}%</div>
                  </div>
                  <span className="num t-sm" style={{ fontWeight: 620, color: buy ? 'hsl(var(--gain))' : 'hsl(var(--warn))' }}>{buy ? '+' : '−'}{money(Math.abs(d.deltaValue), true)}</span>
                </div>
              );
            })}
            {actions.length > 0 && <Button variant="primary" icon="scale" style={{ marginTop: 16, alignSelf: 'flex-start' }} onClick={() => go('planner')}>Susun rencana rebalancing</Button>}
          </div>
        </div>

        <div className="card">
          <div className="card-head"><div><div className="card-title">Kesehatan Portofolio</div><div className="card-sub">indikator risiko & ketahanan</div></div></div>
          <div className="card-pad" style={{ paddingTop: 6 }}>
            <MetricRow icon="shield" label="Dana darurat" status={runwayStatus} value={P.RUNWAY_MONTHS.toFixed(1).replace('.', ',') + ' bln'} hint={`Aset likuid ${money(P.LIQUID, true)} · menutup ${P.RUNWAY_MONTHS.toFixed(0)}× pengeluaran`} />
            <MetricRow icon="pie" label="Konsentrasi terbesar" status={concStatus} value={P.formatPct(P.TOP_HOLDING_PCT, { digits: 1 }).replace('+', '')} hint={`${P.TOP_HOLDING.symbol} — posisi tunggal terbesar`} />
            <MetricRow icon="scale" label="Tingkat menabung" status={saveStatus} value={P.formatPct(P.SAVINGS_RATE, { digits: 0 }).replace('+', '')} hint="dari pemasukan bulan ini" />
            <div className="flex items-center gap-3" style={{ padding: '12px 0' }}>
              <span style={{ width: 36, height: 36, borderRadius: 10, flexShrink: 0, display: 'grid', placeItems: 'center', color: STT[driftStatus].color, background: STT[driftStatus].soft }}><Icon name="target" size={17} /></span>
              <div className="flex-1">
                <div className="flex items-center justify-between gap-2">
                  <span className="t-sm" style={{ fontWeight: 540 }}>Diversifikasi</span>
                  <span className="num t-sm" style={{ fontWeight: 620, color: STT[driftStatus].color }}>{P.ALLOC.length} kelas · {P.HOLDINGS.length} aset</span>
                </div>
                <div className="t-xs t-muted">{actions.length} kategori perlu penyesuaian</div>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* ===== composition over time ===== */}
      <div className="card">
        <div className="card-head">
          <div><div className="card-title">Komposisi Kekayaan</div><div className="card-sub">nilai pasar per kelas aset (IDR)</div></div>
          <Seg value={range} onChange={setRange} options={[{ value: '3m', label: '3B' }, { value: '6m', label: '6B' }, { value: '1y', label: '1Th' }]} />
        </div>
        <div className="card-pad" style={{ paddingTop: 10 }}>
          {loading ? <Skeleton w="100%" h={260} /> : (
            <>
              <StackedArea data={stackSlice} order={P.STACK_ORDER} colorFn={P.catColor} formatY={(v) => P.formatIDR(v, { compact: true }).replace('Rp ', '')} />
              <div className="flex gap-4 flex-wrap" style={{ marginTop: 10, paddingLeft: 56 }}>
                {P.STACK_ORDER.slice().reverse().map(id => (
                  <span key={id} className="flex items-center gap-2 t-xs t-muted"><span className="dot" style={{ width: 8, height: 8, background: P.catColor(id) }}></span>{P.catLabel(id)}</span>
                ))}
              </div>
            </>
          )}
        </div>
      </div>

      {/* ===== movers + goals ===== */}
      <div className="grid gap-5" style={{ gridTemplateColumns: 'minmax(0,1fr) minmax(0,1fr)' }}>
        <div className="card">
          <div className="card-head"><div><div className="card-title">Pergerakan Hari Ini</div><div className="card-sub">kontributor terbesar</div></div></div>
          <div className="card-pad flex col gap-4" style={{ paddingTop: 14 }}>
            <div>
              <div className="t-xs t-label" style={{ marginBottom: 6, color: 'hsl(var(--gain))' }}>Penguat</div>
              {P.GAINERS.map(h => <MoverRow key={h.id} h={h} money={money} />)}
            </div>
            <div>
              <div className="t-xs t-label" style={{ marginBottom: 6, color: 'hsl(var(--loss))' }}>Pelemah</div>
              {P.LOSERS.map(h => <MoverRow key={h.id} h={h} money={money} />)}
            </div>
          </div>
        </div>

        <div className="card">
          <div className="card-head"><div><div className="card-title">Tujuan Keuangan</div><div className="card-sub">progres menuju target</div></div></div>
          <div className="card-pad flex col gap-5" style={{ paddingTop: 16 }}>
            {P.GOALS.map(g => {
              const pct = Math.min(100, (g.current / g.target) * 100);
              const done = pct >= 100;
              return (
                <div key={g.id} className="flex col gap-2">
                  <div className="flex items-center gap-3">
                    <span style={{ width: 34, height: 34, borderRadius: 9, flexShrink: 0, display: 'grid', placeItems: 'center', background: P.catColor(g.cat) + '22', color: P.catColor(g.cat) }}><Icon name={g.icon} size={17} /></span>
                    <div className="flex-1" style={{ minWidth: 0 }}>
                      <div className="t-sm" style={{ fontWeight: 600 }}>{g.label}</div>
                      <div className="t-xs t-muted">{g.sub}</div>
                    </div>
                    <span className="num t-sm" style={{ fontWeight: 620 }}>{pct.toFixed(0)}%</span>
                  </div>
                  <Progress value={pct} color={P.catColor(g.cat)} />
                  <div className="flex items-center justify-between t-xs t-muted num">
                    <span>{money(g.current, true)}</span>
                    <span>target {money(g.target, true)}</span>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}

function MoverRow({ h, money }) {
  const P = window.PT;
  const up = h.dayPct >= 0;
  return (
    <div className="flex items-center gap-3" style={{ padding: '8px 0' }}>
      <span className="dot" style={{ background: P.catColor(h.cat), width: 9, height: 9 }}></span>
      <div className="flex-1" style={{ minWidth: 0 }}>
        <div className="t-sm" style={{ fontWeight: 580 }}>{h.symbol}</div>
        <div className="t-xs t-muted truncate" style={{ maxWidth: 160 }}>{h.name}</div>
      </div>
      <div className="text-right">
        <div className={'num t-sm ' + (up ? 'gain' : 'loss')} style={{ fontWeight: 600 }}>{P.formatPct(h.dayPct, { digits: 2 })}</div>
        <div className="t-xs t-muted num">{money(h.mv, true)}</div>
      </div>
    </div>
  );
}
window.Dashboard = Dashboard;
window.MoverRow = MoverRow;
