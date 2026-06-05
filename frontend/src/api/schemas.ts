import { z } from "zod";

export const AccountSchema = z.object({
  id: z.number(),
  name: z.string(),
  account_type: z.string(),
  institution: z.string().nullable().optional(),
  native_currency: z.string(),
  note: z.string().nullable().optional(),
  created_at: z.string(),
});
export type Account = z.infer<typeof AccountSchema>;

export const CategorySchema = z.object({
  id: z.number(),
  name: z.string(),
  target_pct: z.string(),
  tolerance_band_pct: z.string().nullable().optional(),
  sort_order: z.number(),
  color: z.string().nullable().optional(),
});
export type Category = z.infer<typeof CategorySchema>;

export const InstrumentSchema = z.object({
  id: z.number(),
  symbol: z.string(),
  name: z.string(),
  instrument_type: z.string(),
  native_currency: z.string(),
  category_id: z.number().nullable().optional(),
  price_source: z.string(),
  decimals: z.number(),
  note: z.string().nullable().optional(),
});
export type Instrument = z.infer<typeof InstrumentSchema>;

export const TransactionSchema = z.object({
  id: z.number(),
  account_id: z.number(),
  instrument_id: z.number(),
  txn_type: z.string(),
  executed_at: z.string(),
  quantity: z.string(),
  price_native: z.string(),
  fee_native: z.string(),
  currency: z.string(),
  fx_to_idr: z.string(),
  fx_to_usd: z.string(),
  note: z.string().nullable().optional(),
});
export type Transaction = z.infer<typeof TransactionSchema>;

export const PositionSchema = z.object({
  instrument_id: z.number(),
  quantity: z.string(),
  avg_cost: z.string(),
  cost_basis_total: z.string(),
  latest_price: z.string(),
  price_stale: z.boolean(),
  market_value_native: z.string(),
  market_value_idr: z.string(),
  market_value_usd: z.string(),
  unrealized_pnl: z.string(),
  realized_pnl: z.string(),
  income: z.string(),
  cost_basis_idr_total: z.string(),
  unrealized_pnl_idr: z.string(),
  unrealized_price_pnl_idr: z.string(),
  unrealized_fx_pnl_idr: z.string(),
  realized_pnl_idr: z.string(),
  realized_price_pnl_idr: z.string(),
  realized_fx_pnl_idr: z.string(),
  fx_incomplete: z.boolean(),
});
export type Position = z.infer<typeof PositionSchema>;

export const CategoryAllocationSchema = z.object({
  category_id: z.number(),
  name: z.string(),
  target_pct: z.string(),
  tolerance_band_pct: z.string().nullable().optional(),
  actual_pct: z.string(),
  actual_value_idr: z.string(),
  drift_pct: z.string(),
  out_of_band: z.boolean(),
  rebalance_idr: z.string(),
});
export type CategoryAllocation = z.infer<typeof CategoryAllocationSchema>;

export const PortfolioSummarySchema = z.object({
  net_worth_idr: z.string(),
  net_worth_usd: z.string(),
  total_unrealized_pnl_idr: z.string(),
  total_realized_pnl_idr: z.string(),
  total_unrealized_price_pnl_idr: z.string(),
  total_unrealized_fx_pnl_idr: z.string(),
  total_realized_price_pnl_idr: z.string(),
  total_realized_fx_pnl_idr: z.string(),
  fx_incomplete: z.boolean(),
  xirr: z.number().nullable(),
  positions: z.array(PositionSchema),
  allocation: z.array(CategoryAllocationSchema),
});
export type PortfolioSummary = z.infer<typeof PortfolioSummarySchema>;

export const SnapshotSchema = z.object({
  as_of: z.string(),
  total_idr: z.string(),
  total_usd: z.string(),
  breakdown_json: z.string(),
  price_pnl_idr: z.string().nullable().optional(),
  fx_pnl_idr: z.string().nullable().optional(),
});
export type Snapshot = z.infer<typeof SnapshotSchema>;

export const ExtractedEntrySchema = z.object({
  entry_type: z.string(),
  symbol: z.string().nullable().optional(),
  instrument_name: z.string().nullable().optional(),
  quantity: z.string().nullable().optional(),
  price_native: z.string().nullable().optional(),
  fee_native: z.string().nullable().optional(),
  currency: z.string().nullable().optional(),
  executed_at: z.string().nullable().optional(),
  account_hint: z.string().nullable().optional(),
  note: z.string().nullable().optional(),
  amount_native: z.string().nullable().optional(),
  confidence: z.number().default(1),
});
export type ExtractedEntry = z.infer<typeof ExtractedEntrySchema>;

export const ReviewItemSchema = z.object({
  id: z.number(),
  batch_id: z.string(),
  source_kind: z.string(),
  source_filename: z.string(),
  source_path: z.string(),
  doc_type: z.string(),
  status: z.string(),
  needs_attention: z.number(),
  payload_json: z.string(),
  raw_llm_json: z.string(),
  suggested_instrument_id: z.number().nullable().optional(),
  suggested_account_id: z.number().nullable().optional(),
  created_txn_id: z.number().nullable().optional(),
  created_at: z.string(),
  confirmed_at: z.string().nullable().optional(),
});
export type ReviewItem = z.infer<typeof ReviewItemSchema>;

export const IngestResultSchema = z.object({
  batch_id: z.string(),
  items: z.array(ReviewItemSchema),
});
export type IngestResult = z.infer<typeof IngestResultSchema>;

export const CashflowCategorySchema = z.object({
  id: z.number(),
  name: z.string(),
  kind: z.string(),
  monthly_budget: z.string().nullable().optional(),
  color: z.string().nullable().optional(),
});
export type CashflowCategory = z.infer<typeof CashflowCategorySchema>;

export const CashflowSchema = z.object({
  id: z.number(),
  account_id: z.number().nullable().optional(),
  occurred_on: z.string(),
  direction: z.string(),
  amount: z.string(),
  currency: z.string(),
  category_id: z.number().nullable().optional(),
  note: z.string().nullable().optional(),
  created_at: z.string(),
});
export type Cashflow = z.infer<typeof CashflowSchema>;

export const MonthSummarySchema = z.object({
  month: z.string(),
  total_in: z.string(),
  total_out: z.string(),
  net: z.string(),
  categories: z.array(
    z.object({
      category_id: z.number().nullable(),
      name: z.string(),
      kind: z.string(),
      actual: z.string(),
      budget: z.string().nullable(),
      over_budget: z.boolean(),
    }),
  ),
});
export type MonthSummary = z.infer<typeof MonthSummarySchema>;

export const ConnectorSchema = z.object({
  id: z.number(),
  account_id: z.number(),
  kind: z.string(),
  label: z.string(),
  config_json: z.string(),
  cursor: z.string().nullable(),
  last_synced_at: z.string().nullable(),
  enabled: z.number(),
  created_at: z.string(),
});
export type Connector = z.infer<typeof ConnectorSchema>;

export const SyncReportSchema = z.object({
  inserted: z.number(),
  staged: z.number(),
  skipped: z.number(),
});
export type SyncReport = z.infer<typeof SyncReportSchema>;

export const ChatMessageSchema = z.object({
  id: z.number(),
  role: z.string(),
  content: z.string(),
  channel: z.string(),
  created_at: z.string(),
});
export type ChatMessage = z.infer<typeof ChatMessageSchema>;

export const ChatReplySchema = z.object({
  reply: z.string(),
});
export type ChatReply = z.infer<typeof ChatReplySchema>;

// ── Phase 5A: Insights endpoint ───────────────────────────────────────────

export const InsightsSchema = z.object({
  /** Net worth in IDR (returned as string from API) */
  net_worth_idr: z.string(),
  /** Day-over-day delta in IDR (string) */
  day_delta_idr: z.string(),
  /** Day-over-day delta as a percentage (string, e.g. "1.25") */
  day_delta_pct: z.string(),
  /** Savings rate, e.g. "0.32" (string) */
  savings_rate: z.string(),
  /** Trailing-twelve-months dividend income in IDR (string) */
  dividend_ttm_idr: z.string(),
  /** Portfolio yield percentage (string) */
  yield_pct: z.string(),
  /** Liquid assets in IDR (string) */
  liquid_idr: z.string(),
  /** Emergency fund runway in months (string — Decimal serialized as string by the API) */
  runway_months: z.string(),
  /** Largest concentration position, or null if empty */
  concentration: z
    .object({ symbol: z.string(), pct: z.string() })
    .nullable(),
  /** Composition time-series for the stacked-area chart */
  composition: z.array(
    z.object({
      as_of: z.string(),
      total_idr: z.string(),
      parts: z.array(
        z.object({ category: z.string(), value_idr: z.string() }),
      ),
    }),
  ),
  /** Monthly income in IDR (string) */
  monthly_income_idr: z.string(),
  /** Monthly expense in IDR (string) */
  monthly_expense_idr: z.string(),
});
export type Insights = z.infer<typeof InsightsSchema>;

// ── Phase 5A: Goals endpoint ──────────────────────────────────────────────

export const GoalSchema = z.object({
  id: z.number(),
  label: z.string(),
  note: z.string().nullable().optional(),
  /** Target amount in IDR (string) */
  target_idr: z.string(),
  /** How the "current" progress is computed: "cash" | "networth" | "manual" */
  current_kind: z.string(),
  /** Current progress amount in IDR (string) */
  current_idr: z.string(),
  sort_order: z.number(),
  created_at: z.string(),
});
export type Goal = z.infer<typeof GoalSchema>;

// ── WhatsApp connection ─────────────────────────────────────────────────────

export const WhatsappStatusSchema = z.object({
  status: z.enum(["disconnected", "connecting", "qr", "connected"]),
  qr: z.string().nullable(),
  number: z.string().nullable(),
});
export type WhatsappStatus = z.infer<typeof WhatsappStatusSchema>;

// ── Telegram connection ─────────────────────────────────────────────────────

export const TelegramStatusSchema = z.object({
  configured: z.boolean(),
  linked: z.boolean(),
  username: z.string().nullable(),
});
export type TelegramStatus = z.infer<typeof TelegramStatusSchema>;

export const TelegramLinkCodeSchema = z.object({
  code: z.string(),
  expires_in: z.number(),
});
export type TelegramLinkCode = z.infer<typeof TelegramLinkCodeSchema>;

export const LoginResponseSchema = z.object({ token: z.string() });
export type LoginResponse = z.infer<typeof LoginResponseSchema>;

// ── Performance analytics (TWR + risk) ────────────────────────────────────

export const PerformanceSchema = z.object({
  base: z.string(),
  points: z.array(
    z.object({ date: z.string(), cum_return: z.number(), nav: z.number() }),
  ),
  metrics: z.object({
    total_return: z.number(),
    // null when annualizing is meaningless (backend: non-finite result)
    annualized: z.number().nullable(),
    max_drawdown: z.number(),
    volatility: z.number(),
  }),
  insufficient_data: z.boolean(),
});
export type Performance = z.infer<typeof PerformanceSchema>;
