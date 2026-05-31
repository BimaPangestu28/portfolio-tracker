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
});
export type Snapshot = z.infer<typeof SnapshotSchema>;
