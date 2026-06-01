/* ============================================================
   Import (review queue) + Connectors + Chat
   ============================================================ */
const { Card: Card, Icon: Icon, Badge: Badge, Button: Button, Dialog: Dialog, Field: Field, Input: Input, Select: Select, Empty: Empty, Textarea: Textarea } = window;
const { useState: oState, useRef: oRef, useEffect: oEffect } = React;

/* ---------------- Import ---------------- */
const DOC_ICON = { screenshot: 'image', pdf: 'fileText', csv: 'fileText' };
const DOC_LABEL = { screenshot: 'Screenshot', pdf: 'PDF', csv: 'CSV' };

function Import({ embedded }) {
  const P = window.PT;
  const [queue, setQueue] = oState(P.IMPORT_QUEUE);
  const [accounts, setAccounts] = oState(P.ACCOUNTS.map(a => a.label));
  const [drag, setDrag] = oState(false);

  const patch = (id, key, val) => setQueue(q => q.map(it => it.id === id ? { ...it, fields: { ...it.fields, [key]: val }, needs: key === 'account' && val ? it.needs && false : it.needs } : it));
  const confirmItem = (it) => {
    if (!it.fields.account) { window.toast('Pilih akun dulu sebelum konfirmasi', 'warn'); return; }
    setQueue(q => q.filter(x => x.id !== it.id)); window.toast(`${it.fields.symbol} dikonfirmasi & masuk transaksi`, 'success');
  };
  const reject = (it) => { setQueue(q => q.filter(x => x.id !== it.id)); window.toast('Item ditolak', 'info'); };
  const addAccount = (id) => {
    const name = (window.prompt && window.prompt('Nama akun baru')) || '';
    if (name) { setAccounts(a => [...a, name]); patch(id, 'account', name); window.toast('Akun dibuat', 'success'); }
  };

  return (
    <div>
      {!embedded && <PageHeader title="Import" sub="Antrian review — tinjau sebelum masuk portofolio" />}

      <div className="card card-pad flex items-center gap-3" style={{ marginBottom: 18, background: 'hsl(var(--accent-soft))', border: '1px solid hsl(var(--primary) / 0.25)' }}>
        <Icon name="info" size={18} style={{ color: 'hsl(var(--primary))', flexShrink: 0 }} />
        <span className="t-sm" style={{ color: 'hsl(var(--primary))' }}>Tidak ada yang otomatis tercatat. Setiap item harus kamu <strong>Konfirmasi</strong> atau <strong>Tolak</strong> dulu.</span>
      </div>

      <div className="grid gap-4" style={{ gridTemplateColumns: 'minmax(0,1fr) minmax(0,1fr)', marginBottom: 22 }}>
        <div className={'card'} style={{ borderStyle: 'dashed', borderColor: drag ? 'hsl(var(--primary))' : 'hsl(var(--border-strong))', background: drag ? 'hsl(var(--accent-soft))' : 'hsl(var(--card))', transition: 'all 0.15s' }}
          onDragOver={e => { e.preventDefault(); setDrag(true); }} onDragLeave={() => setDrag(false)}
          onDrop={e => { e.preventDefault(); setDrag(false); window.toast('Berkas diunggah — diproses oleh LLM…', 'info'); }}>
          <div className="empty" style={{ padding: '34px 24px' }}>
            <div className="empty-icon" style={{ background: 'hsl(var(--accent-soft))', color: 'hsl(var(--primary))' }}><Icon name="upload" size={26} /></div>
            <div><div className="t-h3">Tarik screenshot / PDF ke sini</div><div className="t-sm t-muted" style={{ marginTop: 4 }}>Bukti transaksi diekstrak otomatis dengan LLM</div></div>
            <Button variant="outline" icon="image" size="sm">Pilih berkas</Button>
          </div>
        </div>
        <div className="card">
          <div className="card-head"><div><div className="card-title">Impor CSV</div><div className="card-sub">ekspor dari broker / exchange</div></div></div>
          <div className="card-pad flex col gap-3" style={{ paddingTop: 14 }}>
            <Field label="Sumber"><Select><option>Bibit — riwayat reksadana</option><option>IPOT — trade confirmation</option><option>Pintu — order history</option><option>Custom (auto-map kolom)</option></Select></Field>
            <div className="flex items-center gap-2"><Button variant="outline" icon="fileText" size="sm">Pilih CSV</Button><span className="t-xs t-muted">maks 5MB · dipetakan ke antrian review</span></div>
          </div>
        </div>
      </div>

      <div className="flex items-center justify-between" style={{ marginBottom: 12 }}>
        <h2 className="t-h2">Antrian Review</h2>
        <Badge tone={queue.filter(q => q.needs).length ? 'warn' : 'neutral'}>{queue.filter(q => q.needs).length} perlu perhatian · {queue.length} total</Badge>
      </div>

      {queue.length === 0 ? <Card><Empty icon="checkCircle" title="Antrian bersih" sub="Semua item sudah ditinjau. Unggah bukti baru untuk melanjutkan." /></Card> : (
        <div className="flex col gap-3">
          {queue.map(it => (
            <Card key={it.id} className="card-pad" style={it.needs ? { borderColor: 'hsl(var(--warn) / 0.4)' } : null}>
              <div className="flex items-center gap-3" style={{ marginBottom: 14 }}>
                <span style={{ width: 38, height: 38, borderRadius: 9, display: 'grid', placeItems: 'center', background: 'hsl(var(--muted))', color: 'hsl(var(--muted-foreground))', flexShrink: 0 }}><Icon name={DOC_ICON[it.docType]} size={18} /></span>
                <div className="flex-1" style={{ minWidth: 0 }}>
                  <div className="flex items-center gap-2"><span style={{ fontWeight: 600 }} className="t-sm mono truncate">{it.source}</span><Badge tone="neutral">{DOC_LABEL[it.docType]}</Badge></div>
                  <div className="t-xs t-muted">keyakinan ekstraksi {Math.round(it.conf * 100)}%</div>
                </div>
                {it.needs ? <Badge tone="warn"><Icon name="alertTriangle" size={11} />perlu perhatian</Badge> : <Badge tone="gain"><Icon name="check" size={11} />siap</Badge>}
              </div>
              <div className="grid gap-3" style={{ gridTemplateColumns: 'repeat(auto-fit, minmax(120px,1fr))' }}>
                <Field label="Tipe"><Select value={it.fields.type} onChange={e => patch(it.id, 'type', e.target.value)}>{['buy', 'sell', 'dividend', 'transfer'].map(t => <option key={t} value={t}>{t}</option>)}</Select></Field>
                <Field label="Instrumen"><Input value={it.fields.symbol} onChange={e => patch(it.id, 'symbol', e.target.value)} /></Field>
                <Field label="Jumlah"><Input value={it.fields.qty} onChange={e => patch(it.id, 'qty', e.target.value)} /></Field>
                <Field label="Harga"><Input value={it.fields.price} onChange={e => patch(it.id, 'price', e.target.value)} /></Field>
                <Field label={<span className="flex items-center justify-between">Akun <button className="t-xs" style={{ color: 'hsl(var(--primary))', background: 'none', border: 'none', cursor: 'pointer', padding: 0 }} onClick={() => addAccount(it.id)}>+ baru</button></span>}>
                  <Select value={it.fields.account} onChange={e => patch(it.id, 'account', e.target.value)} style={!it.fields.account ? { borderColor: 'hsl(var(--warn))' } : null}>
                    <option value="">— pilih akun —</option>{accounts.map(a => <option key={a}>{a}</option>)}
                  </Select>
                </Field>
                <Field label="Tanggal"><Input type="date" value={it.fields.date} onChange={e => patch(it.id, 'date', e.target.value)} /></Field>
              </div>
              <div className="flex items-center justify-end gap-2" style={{ marginTop: 14 }}>
                <Button variant="ghost" icon="x" size="sm" onClick={() => reject(it)}>Tolak</Button>
                <Button variant="primary" icon="check" size="sm" onClick={() => confirmItem(it)}>Konfirmasi</Button>
              </div>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
window.Import = Import;

/* ---------------- Connectors ---------------- */
const CONN_ICON = { exchange: 'coins', wallet: 'wallet2', bank: 'landmark' };
const CONN_STATUS = { ok: { tone: 'gain', label: 'Tersinkron' }, stale: { tone: 'warn', label: 'Perlu sync' }, error: { tone: 'loss', label: 'Error' } };

function Connectors({ embedded }) {
  const P = window.PT;
  const [conns, setConns] = oState(P.CONNECTORS);
  const [open, setOpen] = oState(false);
  const [syncing, setSyncing] = oState(null);
  const [form, setForm] = oState({ kind: 'exchange', label: '', apiKey: '' });

  const sync = (c) => {
    setSyncing(c.id);
    setTimeout(() => {
      setSyncing(null);
      setConns(cs => cs.map(x => x.id === c.id ? { ...x, status: 'ok', last: new Date().toISOString() } : x));
      window.toast('Sync selesai · 3 ditambah, 1 di-stage, 2 dilewati', 'success');
    }, 1100);
  };
  const add = () => {
    if (!form.label) { window.toast('Beri nama konektor', 'warn'); return; }
    setConns(cs => [...cs, { id: 'c' + Date.now(), kind: form.kind, label: form.label, status: 'stale', last: new Date().toISOString() }]);
    setOpen(false); setForm({ kind: 'exchange', label: '', apiKey: '' }); window.toast('Konektor ditambahkan', 'success');
  };

  const connActions = <Button variant="primary" icon="plus" size="sm" onClick={() => setOpen(true)}>Tambah Konektor</Button>;
  return (
    <div>
      {embedded
        ? <div className="flex items-center justify-between" style={{ marginBottom: 16, gap: 10, flexWrap: 'wrap' }}><span className="t-sm t-muted">Sinkronisasi on-chain & exchange</span>{connActions}</div>
        : <PageHeader title="Connectors" sub="Sinkronisasi on-chain & exchange" actions={connActions} />}
      <div className="grid gap-4" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(300px,1fr))' }}>
        {conns.map(c => {
          const st = CONN_STATUS[c.status];
          return (
            <Card key={c.id} className="card-pad">
              <div className="flex items-center gap-3" style={{ marginBottom: 16 }}>
                <span style={{ width: 42, height: 42, borderRadius: 11, display: 'grid', placeItems: 'center', background: 'hsl(var(--muted))', color: 'hsl(var(--foreground))', flexShrink: 0 }}><Icon name={CONN_ICON[c.kind] || 'plug'} size={20} /></span>
                <div className="flex-1" style={{ minWidth: 0 }}>
                  <div style={{ fontWeight: 600 }} className="truncate">{c.label}</div>
                  <div className="t-xs t-muted">disinkron {P.relTime(c.last)}</div>
                </div>
                <Badge tone={st.tone} dot>{st.label}</Badge>
              </div>
              <Button variant={c.status === 'error' ? 'outline' : 'outline'} icon="refresh" size="sm" className="w-full" disabled={syncing === c.id} onClick={() => sync(c)} style={{ width: '100%' }}>
                {syncing === c.id ? 'Menyinkron…' : 'Sync sekarang'}
              </Button>
            </Card>
          );
        })}
      </div>

      <Dialog open={open} onClose={() => setOpen(false)} title="Tambah Konektor" sub="Hubungkan exchange atau wallet on-chain"
        footer={<><Button onClick={() => setOpen(false)}>Batal</Button><Button variant="primary" icon="check" onClick={add}>Hubungkan</Button></>}>
        <Field label="Jenis"><Select value={form.kind} onChange={e => setForm(f => ({ ...f, kind: e.target.value }))}><option value="exchange">Exchange</option><option value="wallet">Wallet on-chain</option><option value="bank">Bank</option></Select></Field>
        <Field label="Label"><Input placeholder="mis. Pintu, Ledger ETH…" value={form.label} onChange={e => setForm(f => ({ ...f, label: e.target.value }))} /></Field>
        <Field label="API Key" hint="Disimpan terenkripsi, hanya akses baca."><Input type="password" placeholder="••••••••••••" value={form.apiKey} onChange={e => setForm(f => ({ ...f, apiKey: e.target.value }))} /></Field>
      </Dialog>
    </div>
  );
}
window.Connectors = Connectors;

/* ---------------- DataHub (tabbed: Sinkron + Review impor) ---------------- */
function DataHub() {
  const [tab, setTab] = oState('sync');
  const needs = window.PT.IMPORT_QUEUE.filter(q => q.needs).length;
  return (
    <div>
      <PageHeader title="Data" sub="Sumber data & antrian impor" />
      <div className="ptabs">
        <button className={'ptab' + (tab === 'sync' ? ' active' : '')} onClick={() => setTab('sync')}>Sinkron</button>
        <button className={'ptab' + (tab === 'review' ? ' active' : '')} onClick={() => setTab('review')}>Review impor{needs ? ' · ' + needs : ''}</button>
      </div>
      {tab === 'sync' ? <Connectors embedded /> : <Import embedded />}
    </div>
  );
}
window.DataHub = DataHub;

/* ---------------- Chat ---------------- */
function mdInline(text) {
  const esc = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  return esc.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
}
const CHAT_REPLIES = [
  'Ada **2 kategori di luar batas**: **Crypto 32,7%** vs target 28% (pangkas ±Rp 72 jt) dan **Saham IDX 13,7%** vs target 20% (beli ±Rp 96 jt). Memindahkan dari Crypto ke Saham IDX akan mengembalikan keduanya ke target.',
  'Performa terbaik: **EMAS +21,5%** dan **BTC +20,5%**. Yang menahan: **GOTO −11,5%** dan **TLKM −9,8%**.',
  'XIRR portofolio **+18,4%** per tahun — di atas IHSG (~9%) untuk periode yang sama, terutama berkat eksposur **crypto & emas**.',
];

function Chat() {
  const P = window.PT;
  const [msgs, setMsgs] = oState(P.CHAT_SEED);
  const [text, setText] = oState('');
  const [thinking, setThinking] = oState(false);
  const scrollRef = oRef(null);
  oEffect(() => { if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight; }, [msgs, thinking]);

  const send = () => {
    if (!text.trim()) return;
    const q = text.trim();
    setMsgs(m => [...m, { role: 'user', text: q }]); setText(''); setThinking(true);
    setTimeout(() => {
      setThinking(false);
      setMsgs(m => [...m, { role: 'assistant', text: CHAT_REPLIES[Math.floor(Math.random() * CHAT_REPLIES.length)] }]);
    }, 1400);
  };
  const suggestions = ['Apakah saya perlu rebalancing?', 'Performa terbaik bulan ini?', 'Berapa XIRR saya?'];

  return (
    <div className="flex col" style={{ height: 'calc(100vh - 64px - 48px)', maxHeight: 760 }}>
      <div className="flex items-center justify-between" style={{ marginBottom: 12 }}>
        <div><h1 className="t-h1">Chat</h1><div className="t-sm t-muted">Tanya jawab portofolio</div></div>
        <Badge tone="gain" dot>Sinkron dengan WhatsApp</Badge>
      </div>
      <Card className="flex col flex-1" style={{ overflow: 'hidden' }}>
        <div ref={scrollRef} className="flex col gap-4 flex-1" style={{ overflowY: 'auto', padding: '22px' }}>
          {msgs.map((m, i) => (
            <div key={i} className="flex gap-3" style={{ flexDirection: m.role === 'user' ? 'row-reverse' : 'row', alignItems: 'flex-start' }}>
              <span style={{ width: 30, height: 30, borderRadius: 8, flexShrink: 0, display: 'grid', placeItems: 'center', background: m.role === 'user' ? 'hsl(var(--muted))' : 'linear-gradient(150deg, hsl(var(--primary)), hsl(262 83% 64%))', color: m.role === 'user' ? 'hsl(var(--muted-foreground))' : '#fff' }}>
                <Icon name={m.role === 'user' ? 'wallet2' : 'sparkles'} size={16} />
              </span>
              <div style={{ maxWidth: '76%', padding: '11px 14px', borderRadius: 14, fontSize: 13.5, lineHeight: 1.55,
                background: m.role === 'user' ? 'hsl(var(--primary))' : 'hsl(var(--muted))',
                color: m.role === 'user' ? 'hsl(var(--primary-foreground))' : 'hsl(var(--foreground))',
                borderTopRightRadius: m.role === 'user' ? 4 : 14, borderTopLeftRadius: m.role === 'user' ? 14 : 4 }}
                dangerouslySetInnerHTML={{ __html: mdInline(m.text) }}></div>
            </div>
          ))}
          {thinking && (
            <div className="flex gap-3 items-start">
              <span style={{ width: 30, height: 30, borderRadius: 8, display: 'grid', placeItems: 'center', background: 'linear-gradient(150deg, hsl(var(--primary)), hsl(262 83% 64%))', color: '#fff' }}><Icon name="sparkles" size={16} /></span>
              <div style={{ padding: '13px 16px', borderRadius: 14, borderTopLeftRadius: 4, background: 'hsl(var(--muted))' }}>
                <div className="flex gap-1">
                  <span className="think-dot"></span><span className="think-dot" style={{ animationDelay: '0.15s' }}></span><span className="think-dot" style={{ animationDelay: '0.3s' }}></span>
                </div>
              </div>
            </div>
          )}
        </div>
        <div style={{ borderTop: '1px solid hsl(var(--border))', padding: '14px 18px' }}>
          <div className="flex gap-2" style={{ marginBottom: 10, flexWrap: 'wrap' }}>
            {suggestions.map(s => <button key={s} className="chip" style={{ cursor: 'pointer' }} onClick={() => { setText(s); }}>{s}</button>)}
          </div>
          <div className="flex gap-2 items-end">
            <Textarea rows={1} placeholder="Tanya tentang portofolio…" value={text} onChange={e => setText(e.target.value)} onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); } }} style={{ flex: 1 }} />
            <Button variant="primary" onClick={send} disabled={!text.trim()} style={{ width: 42, padding: 0, height: 42 }}><Icon name="send" size={17} /></Button>
          </div>
        </div>
      </Card>
    </div>
  );
}
window.Chat = Chat;
