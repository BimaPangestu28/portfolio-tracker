/* ============================================================
   Portfolio Tracker — sample data + format helpers
   (money/qty kept as strings, parsed only for display)
   ============================================================ */
(function () {
  const USD_RATE = 16250; // IDR per USD (display fx)

  // ---- formatters (mirror src/lib/format.ts behavior) ----
  function formatIDR(value, opts) {
    const n = typeof value === 'string' ? parseFloat(value) : value;
    if (n == null || isNaN(n)) return 'Rp 0';
    const compact = opts && opts.compact;
    if (compact && Math.abs(n) >= 1e9) return 'Rp ' + (n / 1e9).toFixed(2).replace('.', ',') + ' M';
    if (compact && Math.abs(n) >= 1e6) return 'Rp ' + (n / 1e6).toFixed(1).replace('.', ',') + ' jt';
    return new Intl.NumberFormat('id-ID', { style: 'currency', currency: 'IDR', maximumFractionDigits: 0 }).format(n);
  }
  function formatUSD(value) {
    const n = typeof value === 'string' ? parseFloat(value) : value;
    if (n == null || isNaN(n)) return '$0.00';
    return new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD', maximumFractionDigits: 2 }).format(n);
  }
  function formatPct(value, opts) {
    const n = typeof value === 'string' ? parseFloat(value) : value;
    if (n == null || isNaN(n)) return '0%';
    const sign = n > 0 ? '+' : '';
    const d = opts && opts.digits != null ? opts.digits : 2;
    return sign + n.toFixed(d).replace('.', ',') + '%';
  }
  function formatQty(value) {
    const n = typeof value === 'string' ? parseFloat(value) : value;
    if (n == null || isNaN(n)) return '0';
    return new Intl.NumberFormat('id-ID', { maximumFractionDigits: 8 }).format(n);
  }
  function idrToUsd(n) { return (typeof n === 'string' ? parseFloat(n) : n) / USD_RATE; }
  function relTime(iso) {
    const diff = (Date.now() - new Date(iso).getTime()) / 1000;
    if (diff < 60) return 'baru saja';
    if (diff < 3600) return Math.floor(diff / 60) + ' mnt lalu';
    if (diff < 86400) return Math.floor(diff / 3600) + ' jam lalu';
    return Math.floor(diff / 86400) + ' hari lalu';
  }

  // ---- categorical palette keyed by category id ----
  const CATEGORIES = [
    { id: 'saham',     label: 'Saham IDX',   color: 'var(--cat-saham)' },
    { id: 'crypto',    label: 'Crypto',      color: 'var(--cat-crypto)' },
    { id: 'reksadana', label: 'Reksadana',   color: 'var(--cat-reksadana)' },
    { id: 'obligasi',  label: 'Obligasi/SBN',color: 'var(--cat-obligasi)' },
    { id: 'emas',      label: 'Emas',        color: 'var(--cat-emas)' },
    { id: 'kas',       label: 'Kas & Setara',color: 'var(--cat-kas)' },
  ];
  const catColor = (id) => 'hsl(' + ({
    saham: 'var(--cat-saham)', crypto: 'var(--cat-crypto)', reksadana: 'var(--cat-reksadana)',
    obligasi: 'var(--cat-obligasi)', emas: 'var(--cat-emas)', kas: 'var(--cat-kas)',
  }[id] || 'var(--cat-lainnya)') + ')';
  const catLabel = (id) => (CATEGORIES.find(c => c.id === id) || {}).label || id;

  // ---- holdings (market values in IDR) ----
  const HOLDINGS = [
    { id: 'h1', symbol: 'BBCA',  name: 'Bank Central Asia',    cat: 'saham',     qty: '4200',     avg: '8850',      last: '10325',    mv: 43365000,   stale: false, ts: '2026-06-01T08:55:00Z' },
    { id: 'h2', symbol: 'BBRI',  name: 'Bank Rakyat Indonesia',cat: 'saham',     qty: '9800',     avg: '4120',      last: '4680',     mv: 45864000,   stale: false, ts: '2026-06-01T08:55:00Z' },
    { id: 'h3', symbol: 'TLKM',  name: 'Telkom Indonesia',     cat: 'saham',     qty: '14500',    avg: '3280',      last: '2960',     mv: 42920000,   stale: false, ts: '2026-06-01T08:55:00Z' },
    { id: 'h4', symbol: 'ASII',  name: 'Astra International',  cat: 'saham',     qty: '8000',     avg: '5100',      last: '5825',     mv: 46600000,   stale: false, ts: '2026-06-01T08:55:00Z' },
    { id: 'h5', symbol: 'GOTO',  name: 'GoTo Gojek Tokopedia', cat: 'saham',     qty: '420000',   avg: '78',        last: '69',       mv: 28980000,   stale: true,  ts: '2026-05-30T08:55:00Z' },
    { id: 'h6', symbol: 'BTC',   name: 'Bitcoin',              cat: 'crypto',    qty: '0.215',    avg: '948000000', last: '1142000000',mv: 245530000, stale: false, ts: '2026-06-01T09:02:00Z' },
    { id: 'h7', symbol: 'ETH',   name: 'Ethereum',             cat: 'crypto',    qty: '2.8',      avg: '52400000',  last: '61200000', mv: 171360000,  stale: false, ts: '2026-06-01T09:02:00Z' },
    { id: 'h8', symbol: 'SOL',   name: 'Solana',               cat: 'crypto',    qty: '38',       avg: '2380000',   last: '2120000',  mv: 80560000,   stale: false, ts: '2026-06-01T09:02:00Z' },
    { id: 'h9', symbol: 'RDPU',  name: 'Reksadana Pasar Uang Sucorinvest', cat: 'reksadana', qty: '142000', avg: '1685', last: '1742', mv: 247364000, stale: false, ts: '2026-05-31T17:00:00Z' },
    { id: 'h10',symbol: 'RDSAHAM',name:'Reksadana Saham Schroder',         cat: 'reksadana', qty: '85000',  avg: '1420', last: '1538', mv: 130730000, stale: false, ts: '2026-05-31T17:00:00Z' },
    { id: 'h11',symbol: 'ORI025',name: 'ORI Seri 025 (SBN Ritel)',        cat: 'obligasi',  qty: '180',    avg: '1000000',last:'1012000',mv: 182160000, stale: false, ts: '2026-05-31T17:00:00Z' },
    { id: 'h12',symbol: 'EMAS',  name: 'Emas Antam (gram)',    cat: 'emas',      qty: '95',       avg: '1085000',   last: '1318000',  mv: 125210000,  stale: false, ts: '2026-06-01T07:00:00Z' },
    { id: 'h13',symbol: 'IDR',   name: 'Rupiah (Jago)',        cat: 'kas',       qty: '84500000', avg: '1',         last: '1',        mv: 84500000,   stale: false, ts: '2026-06-01T09:00:00Z' },
    { id: 'h14',symbol: 'USDC',  name: 'USD Coin (cash)',      cat: 'kas',       qty: '2700',     avg: '16100',     last: '16250',    mv: 43875000,   stale: false, ts: '2026-06-01T09:02:00Z' },
  ];
  HOLDINGS.forEach(h => {
    h.cost = parseFloat(h.qty) * parseFloat(h.avg);
    h.upl = h.mv - h.cost;
    h.uplPct = h.cost ? (h.upl / h.cost) * 100 : 0;
  });

  const NET_WORTH = HOLDINGS.reduce((s, h) => s + h.mv, 0);
  const TOTAL_COST = HOLDINGS.reduce((s, h) => s + h.cost, 0);
  const UNREALIZED = NET_WORTH - TOTAL_COST;
  const REALIZED = 38240000;
  const XIRR = 18.4;
  const DAY_DELTA = 9870000;       // +Rp today
  const DAY_DELTA_PCT = 0.61;

  // allocation by category (actual)
  const ALLOC = CATEGORIES.map(c => {
    const value = HOLDINGS.filter(h => h.cat === c.id).reduce((s, h) => s + h.mv, 0);
    return { id: c.id, label: c.label, value, pct: (value / NET_WORTH) * 100 };
  }).filter(a => a.value > 0).sort((a, b) => b.value - a.value);

  // planner targets + tolerance bands
  const TARGETS = [
    { id: 'saham',     target: 20, tol: 5 },
    { id: 'crypto',    target: 28, tol: 4 },
    { id: 'reksadana', target: 25, tol: 4 },
    { id: 'obligasi',  target: 12, tol: 3 },
    { id: 'emas',      target: 7,  tol: 2 },
    { id: 'kas',       target: 8,  tol: 2 },
  ];
  const DRIFT = TARGETS.map(t => {
    const a = ALLOC.find(x => x.id === t.id) || { pct: 0, value: 0 };
    const drift = a.pct - t.target;
    const outOfBand = Math.abs(drift) > t.tol;
    const deltaValue = (t.target / 100) * NET_WORTH - a.value; // +buy / -trim
    return { id: t.id, label: catLabel(t.id), target: t.target, tol: t.tol, actual: a.pct, value: a.value, drift, outOfBand, deltaValue };
  });

  // value history — last 12 months, IDR
  const HISTORY = (function () {
    const base = 1180000000;
    const pts = [];
    const labels = ['Jul','Agu','Sep','Okt','Nov','Des','Jan','Feb','Mar','Apr','Mei','Jun'];
    let v = base;
    const steps = [0.018, 0.031, -0.012, 0.027, 0.041, -0.022, 0.035, 0.052, -0.018, 0.039, 0.028, 0.024];
    for (let i = 0; i < 12; i++) { v = i === 11 ? NET_WORTH : v * (1 + steps[i]); pts.push({ label: labels[i], value: Math.round(v) }); }
    return pts;
  })();

  const ACCOUNTS = [
    { id: 'a1', label: 'IPOT (Indo Premier)', kind: 'broker' },
    { id: 'a2', label: 'Bibit', kind: 'reksadana' },
    { id: 'a3', label: 'Pintu', kind: 'exchange' },
    { id: 'a4', label: 'Ledger (on-chain)', kind: 'wallet' },
    { id: 'a5', label: 'Bank Jago', kind: 'bank' },
  ];

  const TRANSACTIONS = [
    { id: 't1', date: '2026-05-30', type: 'buy',      symbol: 'BBRI',  account: 'IPOT (Indo Premier)', qty: '1200', price: '4680', fee: '14040', ccy: 'IDR', total: 5630040 },
    { id: 't2', date: '2026-05-28', type: 'dividend', symbol: 'BBCA',  account: 'IPOT (Indo Premier)', qty: '4200', price: '205',  fee: '0',     ccy: 'IDR', total: 861000 },
    { id: 't3', date: '2026-05-26', type: 'buy',      symbol: 'ETH',   account: 'Pintu',               qty: '0.5',  price: '60800000',fee: '91200',ccy: 'IDR', total: 30491200 },
    { id: 't4', date: '2026-05-22', type: 'sell',     symbol: 'GOTO',  account: 'IPOT (Indo Premier)', qty: '120000',price: '74',  fee: '8880',  ccy: 'IDR', total: 8871120 },
    { id: 't5', date: '2026-05-20', type: 'buy',      symbol: 'EMAS',  account: 'Bank Jago',           qty: '10',   price: '1305000',fee: '0',    ccy: 'IDR', total: 13050000 },
    { id: 't6', date: '2026-05-15', type: 'transfer', symbol: 'USDC',  account: 'Ledger (on-chain)',   qty: '500',  price: '16200',fee: '32400', ccy: 'IDR', total: 8100000 },
    { id: 't7', date: '2026-05-12', type: 'buy',      symbol: 'RDPU',  account: 'Bibit',               qty: '30000',price: '1738',fee: '0',     ccy: 'IDR', total: 52140000 },
    { id: 't8', date: '2026-05-08', type: 'fee',      symbol: 'BTC',   account: 'Pintu',               qty: '0',    price: '0',   fee: '45000', ccy: 'IDR', total: 45000 },
  ];

  // budget — current month
  const BUDGET_MONTH = '2026-05';
  const BUDGET_KPIS = { income: 38500000, expense: 22640000, net: 15860000 };
  const BUDGET_CATS = [
    { id: 'b1', label: 'Makan & Groceries', spent: 4875000,  budget: 5000000 },
    { id: 'b2', label: 'Transport & BBM',   spent: 1840000,  budget: 2000000 },
    { id: 'b3', label: 'Tagihan & Utilitas',spent: 3120000,  budget: 3000000 },
    { id: 'b4', label: 'Cicilan KPR',       spent: 7200000,  budget: 7200000 },
    { id: 'b5', label: 'Hiburan & Langganan',spent: 1560000, budget: 1200000 },
    { id: 'b6', label: 'Kesehatan',         spent: 640000,   budget: 1500000 },
    { id: 'b7', label: 'Lain-lain',         spent: 3405000,  budget: 3000000 },
  ];
  const CASHFLOW = [
    { id: 'c1', date: '2026-05-29', label: 'Gaji Mei',           cat: 'Pendapatan', amount: 32000000, kind: 'in' },
    { id: 'c2', date: '2026-05-28', label: 'Belanja Superindo',  cat: 'Makan & Groceries', amount: 845000, kind: 'out' },
    { id: 'c3', date: '2026-05-27', label: 'Cicilan KPR BCA',    cat: 'Cicilan KPR', amount: 7200000, kind: 'out' },
    { id: 'c4', date: '2026-05-26', label: 'Freelance design',   cat: 'Pendapatan', amount: 6500000, kind: 'in' },
    { id: 'c5', date: '2026-05-25', label: 'Listrik PLN + Air',  cat: 'Tagihan & Utilitas', amount: 1240000, kind: 'out' },
    { id: 'c6', date: '2026-05-24', label: 'Pertamina',          cat: 'Transport & BBM', amount: 350000, kind: 'out' },
  ];

  // import review queue
  const IMPORT_QUEUE = [
    { id: 'i1', docType: 'screenshot', source: 'Pintu_2026-05-30.png', needs: true,  fields: { type: 'buy', symbol: 'SOL', qty: '5', price: '2118000', account: '', date: '2026-05-30' }, conf: 0.72 },
    { id: 'i2', docType: 'screenshot', source: 'IPOT_trade_0530.jpg',  needs: false, fields: { type: 'buy', symbol: 'ASII', qty: '500', price: '5820', account: 'IPOT (Indo Premier)', date: '2026-05-30' }, conf: 0.94 },
    { id: 'i3', docType: 'csv',        source: 'bibit_export.csv',     needs: false, fields: { type: 'buy', symbol: 'RDSAHAM', qty: '8000', price: '1538', account: 'Bibit', date: '2026-05-29' }, conf: 0.98 },
    { id: 'i4', docType: 'pdf',        source: 'eStatement_Apr.pdf',   needs: true,  fields: { type: 'dividend', symbol: 'TLKM', qty: '14500', price: '0', account: '', date: '2026-05-12' }, conf: 0.61 },
  ];

  const CONNECTORS = [
    { id: 'cn1', kind: 'exchange', label: 'Pintu',  status: 'ok',    last: '2026-06-01T09:02:00Z' },
    { id: 'cn2', kind: 'wallet',   label: 'Ledger (ETH mainnet)', status: 'ok', last: '2026-06-01T08:40:00Z' },
    { id: 'cn3', kind: 'wallet',   label: 'Phantom (Solana)', status: 'stale', last: '2026-05-29T11:20:00Z' },
    { id: 'cn4', kind: 'exchange', label: 'Tokocrypto', status: 'error', last: '2026-05-27T14:05:00Z' },
  ];

  // ---- planner-grade derived metrics ----
  const DAY_CHG = { BBCA: 0.8, BBRI: 1.4, TLKM: -0.6, ASII: 0.5, GOTO: -3.2, BTC: 2.1, ETH: 1.6, SOL: -2.4, RDPU: 0.02, RDSAHAM: 0.7, ORI025: 0.1, EMAS: 1.1, IDR: 0, USDC: 0.1 };
  HOLDINGS.forEach(h => { h.dayPct = DAY_CHG[h.symbol] || 0; h.dayVal = h.mv * h.dayPct / 100; });
  const GAINERS = [...HOLDINGS].sort((a, b) => b.dayPct - a.dayPct).slice(0, 3);
  const LOSERS = [...HOLDINGS].sort((a, b) => a.dayPct - b.dayPct).slice(0, 3);

  const MONTHLY_EXPENSE = BUDGET_KPIS.expense;
  const SAVINGS_RATE = (BUDGET_KPIS.net / BUDGET_KPIS.income) * 100;
  const DIVIDEND_TTM = 24800000;
  const YIELD_PCT = (DIVIDEND_TTM / NET_WORTH) * 100;
  const LIQUID = HOLDINGS.filter(h => h.cat === 'kas').reduce((s, h) => s + h.mv, 0) + (HOLDINGS.find(h => h.symbol === 'RDPU') || { mv: 0 }).mv;
  const RUNWAY_MONTHS = LIQUID / MONTHLY_EXPENSE;
  const TOP_HOLDING = [...HOLDINGS].sort((a, b) => b.mv - a.mv)[0];
  const TOP_HOLDING_PCT = (TOP_HOLDING.mv / NET_WORTH) * 100;

  const GOALS = [
    { id: 'g1', label: 'Dana Darurat', sub: '6× pengeluaran bulanan', icon: 'shield', cat: 'kas', target: MONTHLY_EXPENSE * 6, current: HOLDINGS.filter(h => h.cat === 'kas').reduce((s, h) => s + h.mv, 0) },
    { id: 'g2', label: 'Kebebasan Finansial', sub: '25× pengeluaran tahunan (FIRE)', icon: 'target', cat: 'crypto', target: MONTHLY_EXPENSE * 12 * 25, current: NET_WORTH },
    { id: 'g3', label: 'DP Rumah Kedua', sub: 'target 2028', icon: 'landmark', cat: 'obligasi', target: 500000000, current: 182160000 },
  ];

  // net-worth composition over time (stacked, evolving allocation)
  const STACK = (function () {
    const order = ['kas', 'obligasi', 'emas', 'reksadana', 'saham', 'crypto'];
    const startFr = { kas: 0.09, obligasi: 0.14, emas: 0.10, reksadana: 0.27, saham: 0.18, crypto: 0.22 };
    const endFr = {}; order.forEach(id => { endFr[id] = (ALLOC.find(a => a.id === id) || { pct: 0 }).pct / 100; });
    return HISTORY.map((pt, i) => {
      const t = i / (HISTORY.length - 1);
      let fr = {}, sum = 0;
      order.forEach(id => { fr[id] = startFr[id] + (endFr[id] - startFr[id]) * t; sum += fr[id]; });
      return { label: pt.label, total: pt.value, parts: order.map(id => ({ id, value: Math.round(pt.value * fr[id] / sum) })) };
    });
  })();
  const STACK_ORDER = ['kas', 'obligasi', 'emas', 'reksadana', 'saham', 'crypto'];

  const CHAT_SEED = [
    { role: 'assistant', text: 'Halo! Saya asisten portofolio kamu. Tanyakan apa saja — alokasi, performa, atau ide rebalancing. Jawaban yang sama juga tersedia lewat **WhatsApp** (gateway Baileys).' },
    { role: 'user', text: 'Berapa total kekayaan bersih saya sekarang?' },
    { role: 'assistant', text: 'Net worth kamu saat ini **Rp 1,52 M** (~$93.478). Naik **+0,61%** hari ini (+Rp 9,87 jt). Unrealized P&L **+Rp 108 jt (+7,68%)**, dengan **Crypto** sebagai kontributor terbesar (**32,7%**).' },
  ];

  window.PT = {
    USD_RATE, formatIDR, formatUSD, formatPct, formatQty, idrToUsd, relTime,
    CATEGORIES, catColor, catLabel,
    HOLDINGS, NET_WORTH, TOTAL_COST, UNREALIZED, REALIZED, XIRR, DAY_DELTA, DAY_DELTA_PCT,
    ALLOC, TARGETS, DRIFT, HISTORY, ACCOUNTS, TRANSACTIONS,
    BUDGET_MONTH, BUDGET_KPIS, BUDGET_CATS, CASHFLOW,
    IMPORT_QUEUE, CONNECTORS, CHAT_SEED,
    GAINERS, LOSERS, MONTHLY_EXPENSE, SAVINGS_RATE, DIVIDEND_TTM, YIELD_PCT,
    LIQUID, RUNWAY_MONTHS, TOP_HOLDING, TOP_HOLDING_PCT, GOALS, STACK, STACK_ORDER,
  };
})();
