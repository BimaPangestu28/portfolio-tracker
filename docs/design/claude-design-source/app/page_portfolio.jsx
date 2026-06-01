/* ============================================================
   Holdings + Transactions
   ============================================================ */
const { Card: Card, Icon: Icon, Badge: Badge, Button: Button, Skeleton: Skeleton, Empty: Empty, Dialog: Dialog, Field: Field, Input: Input, Select: Select } = window;
const { useState: hState } = React;

function PageHeader({ title, sub, actions }) {
  return (
    <div className="flex items-center justify-between gap-3" style={{ marginBottom: 18, flexWrap: 'wrap' }}>
      <div>
        <h1 className="t-h1">{title}</h1>
        {sub && <div className="t-sm t-muted" style={{ marginTop: 2 }}>{sub}</div>}
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </div>
  );
}
window.PageHeader = PageHeader;

/* ---------------- Holdings ---------------- */
function Holdings({ loading, embedded }) {
  const P = window.PT;
  const [sort, setSort] = hState({ key: 'mv', dir: 'desc' });
  const rows = [...P.HOLDINGS].sort((a, b) => {
    let av = a[sort.key], bv = b[sort.key];
    if (sort.key === 'symbol') { av = a.symbol; bv = b.symbol; }
    const r = typeof av === 'string' ? av.localeCompare(bv) : av - bv;
    return sort.dir === 'asc' ? r : -r;
  });
  const th = (key, label, right) => (
    <th className={'sortable ' + (right ? 'r' : '')} onClick={() => setSort(s => ({ key, dir: s.key === key && s.dir === 'desc' ? 'asc' : 'desc' }))}>
      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4, flexDirection: right ? 'row-reverse' : 'row' }}>
        {label}{sort.key === key && <Icon name={sort.dir === 'asc' ? 'arrowUp' : 'arrowDown'} size={12} sw={2.6} />}
      </span>
    </th>
  );
  const totalMv = P.NET_WORTH;
  const sub = `${P.HOLDINGS.length} instrumen · nilai pasar ${P.formatIDR(totalMv, { compact: true })}`;
  const actions = <><Button icon="filter" size="sm">Filter</Button><Button variant="primary" icon="plus" size="sm">Tambah</Button></>;

  return (
    <div>
      {embedded
        ? <div className="flex items-center justify-between" style={{ marginBottom: 16, gap: 10, flexWrap: 'wrap' }}><span className="t-sm t-muted">{sub}</span><div className="flex gap-2 items-center">{actions}</div></div>
        : <PageHeader title="Holdings" sub={sub} actions={actions} />}
      <Card>
        <div className="table-wrap">
          <table className="tbl">
            <thead><tr>
              {th('symbol', 'Instrumen')}
              {th('qty', 'Jumlah', true)}
              {th('avg', 'Avg Cost', true)}
              {th('last', 'Harga Terakhir', true)}
              {th('mv', 'Nilai Pasar', true)}
              {th('upl', 'Unrealized P&L', true)}
            </tr></thead>
            <tbody>
              {loading ? Array.from({ length: 6 }).map((_, i) => (
                <tr key={i}>{Array.from({ length: 6 }).map((_, j) => <td key={j}><Skeleton w={j === 0 ? 150 : 80} h={14} style={{ marginLeft: j ? 'auto' : 0 }} /></td>)}</tr>
              )) : rows.map(h => (
                <tr key={h.id}>
                  <td>
                    <div className="flex items-center gap-3">
                      <span className="dot" style={{ background: P.catColor(h.cat), width: 10, height: 10 }}></span>
                      <div>
                        <div style={{ fontWeight: 600 }}>{h.symbol}</div>
                        <div className="t-xs t-muted truncate" style={{ maxWidth: 180 }}>{h.name}</div>
                      </div>
                    </div>
                  </td>
                  <td className="r num">{P.formatQty(h.qty)}</td>
                  <td className="r num t-muted">{P.formatIDR(h.avg)}</td>
                  <td className="r">
                    <div className="flex items-center justify-end gap-2">
                      {h.stale && <Badge tone="warn"><Icon name="clock" size={11} />stale</Badge>}
                      <span className="num">{P.formatIDR(h.last)}</span>
                    </div>
                  </td>
                  <td className="r num" style={{ fontWeight: 580 }}>{P.formatIDR(h.mv)}</td>
                  <td className="r">
                    <div className={'num ' + (h.upl >= 0 ? 'gain' : 'loss')} style={{ fontWeight: 580 }}>
                      {h.upl >= 0 ? '+' : '−'}{P.formatIDR(Math.abs(h.upl), { compact: true })}
                    </div>
                    <div className={'t-xs num ' + (h.upl >= 0 ? 'gain' : 'loss')}>{P.formatPct(h.uplPct)}</div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Card>
    </div>
  );
}
window.Holdings = Holdings;

/* ---------------- Transactions ---------------- */
const TX_TONE = { buy: 'gain', sell: 'loss', dividend: 'primary', transfer: 'neutral', fee: 'warn' };
const TX_LABEL = { buy: 'Beli', sell: 'Jual', dividend: 'Dividen', transfer: 'Transfer', fee: 'Biaya' };

function Transactions({ loading, embedded }) {
  const P = window.PT;
  const [txs, setTxs] = hState(P.TRANSACTIONS);
  const [open, setOpen] = hState(false);
  const [confirm, setConfirm] = hState(null);
  const [saving, setSaving] = hState(false);
  const [form, setForm] = hState({ type: 'buy', account: P.ACCOUNTS[0].label, symbol: '', qty: '', price: '', fee: '0', ccy: 'IDR', date: '2026-06-01' });
  const set = (k) => (e) => setForm(f => ({ ...f, [k]: e.target.value }));

  const submit = () => {
    if (!form.symbol || !form.qty || !form.price) { window.toast('Lengkapi instrumen, jumlah, dan harga', 'warn'); return; }
    setSaving(true);
    setTimeout(() => {
      const total = parseFloat(form.qty) * parseFloat(form.price) + parseFloat(form.fee || 0);
      setTxs(t => [{ id: 'n' + Date.now(), ...form, total }, ...t]);
      setSaving(false); setOpen(false);
      setForm({ type: 'buy', account: P.ACCOUNTS[0].label, symbol: '', qty: '', price: '', fee: '0', ccy: 'IDR', date: '2026-06-01' });
      window.toast('Transaksi ditambahkan', 'success');
    }, 650);
  };
  const del = () => { setTxs(t => t.filter(x => x.id !== confirm.id)); window.toast('Transaksi dihapus', 'success'); setConfirm(null); };

  return (
    <div>
      {embedded
        ? <div className="flex items-center justify-between" style={{ marginBottom: 16, gap: 10, flexWrap: 'wrap' }}><span className="t-sm t-muted">{`${txs.length} catatan`}</span><Button variant="primary" icon="plus" size="sm" onClick={() => setOpen(true)}>Tambah Transaksi</Button></div>
        : <PageHeader title="Transaksi" sub={`${txs.length} catatan`} actions={<Button variant="primary" icon="plus" size="sm" onClick={() => setOpen(true)}>Tambah Transaksi</Button>} />}
      <Card>
        {txs.length === 0 ? <Empty icon="swap" title="Belum ada transaksi" sub="Tambahkan transaksi pertama untuk mulai melacak portofolio." action={<Button variant="primary" icon="plus" onClick={() => setOpen(true)}>Tambah Transaksi</Button>} /> : (
          <div className="table-wrap">
            <table className="tbl">
              <thead><tr><th>Tanggal</th><th>Tipe</th><th>Instrumen</th><th>Akun</th><th className="r">Jumlah</th><th className="r">Harga</th><th className="r">Total</th><th></th></tr></thead>
              <tbody>
                {txs.map(t => (
                  <tr key={t.id}>
                    <td className="num t-muted">{t.date}</td>
                    <td><Badge tone={TX_TONE[t.type]}>{TX_LABEL[t.type]}</Badge></td>
                    <td style={{ fontWeight: 580 }}>{t.symbol}</td>
                    <td className="t-muted t-sm">{t.account}</td>
                    <td className="r num">{P.formatQty(t.qty)}</td>
                    <td className="r num">{parseFloat(t.price) ? P.formatIDR(t.price) : '—'}</td>
                    <td className="r num" style={{ fontWeight: 580 }}>{P.formatIDR(t.total)}</td>
                    <td className="r"><button className="icon-btn" style={{ width: 30, height: 30 }} onClick={() => setConfirm(t)}><Icon name="trash" size={15} /></button></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>

      <Dialog open={open} onClose={() => setOpen(false)} title="Tambah Transaksi" sub="Catat aktivitas portofolio secara manual"
        footer={<><Button onClick={() => setOpen(false)}>Batal</Button><Button variant="primary" onClick={submit} disabled={saving} icon={saving ? 'refresh' : 'check'}>{saving ? 'Menyimpan…' : 'Simpan'}</Button></>}>
        <div className="grid gap-3" style={{ gridTemplateColumns: '1fr 1fr' }}>
          <Field label="Tipe"><Select value={form.type} onChange={set('type')}>{Object.keys(TX_LABEL).map(k => <option key={k} value={k}>{TX_LABEL[k]}</option>)}</Select></Field>
          <Field label="Akun"><Select value={form.account} onChange={set('account')}>{P.ACCOUNTS.map(a => <option key={a.id}>{a.label}</option>)}</Select></Field>
        </div>
        <Field label="Instrumen"><Input placeholder="BBCA, BTC, RDPU…" value={form.symbol} onChange={set('symbol')} /></Field>
        <div className="grid gap-3" style={{ gridTemplateColumns: '1fr 1fr 1fr' }}>
          <Field label="Jumlah"><Input type="number" placeholder="0" value={form.qty} onChange={set('qty')} /></Field>
          <Field label="Harga"><Input type="number" placeholder="0" value={form.price} onChange={set('price')} /></Field>
          <Field label="Biaya"><Input type="number" placeholder="0" value={form.fee} onChange={set('fee')} /></Field>
        </div>
        <div className="grid gap-3" style={{ gridTemplateColumns: '1fr 1fr' }}>
          <Field label="Mata Uang"><Select value={form.ccy} onChange={set('ccy')}><option>IDR</option><option>USD</option></Select></Field>
          <Field label="Tanggal"><Input type="date" value={form.date} onChange={set('date')} /></Field>
        </div>
      </Dialog>

      <Dialog open={!!confirm} onClose={() => setConfirm(null)} title="Hapus transaksi?" sub={confirm ? `${TX_LABEL[confirm.type]} ${confirm.symbol} · ${confirm.date}` : ''} width={400}
        footer={<><Button onClick={() => setConfirm(null)}>Batal</Button><Button variant="danger" icon="trash" onClick={del}>Hapus</Button></>}>
        <p className="t-sm t-muted" style={{ margin: 0 }}>Tindakan ini tidak dapat dibatalkan. Catatan akan dihapus permanen dari portofolio.</p>
      </Dialog>
    </div>
  );
}
window.Transactions = Transactions;

/* ---------------- Portfolio (tabbed: Holdings + Transaksi) ---------------- */
function Portfolio({ loading }) {
  const [tab, setTab] = hState('holdings');
  return (
    <div>
      <PageHeader title="Portofolio" sub="Aset & aktivitas transaksi" />
      <div className="ptabs">
        <button className={'ptab' + (tab === 'holdings' ? ' active' : '')} onClick={() => setTab('holdings')}>Holdings</button>
        <button className={'ptab' + (tab === 'tx' ? ' active' : '')} onClick={() => setTab('tx')}>Transaksi</button>
      </div>
      {tab === 'holdings' ? <Holdings loading={loading} embedded /> : <Transactions loading={loading} embedded />}
    </div>
  );
}
window.Portfolio = Portfolio;
