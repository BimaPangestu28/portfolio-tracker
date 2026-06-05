-- Unrealized P&L decomposition (price vs FX, in IDR) captured per daily snapshot.
-- Nullable: rows written before this feature have no decomposition and stay NULL.
ALTER TABLE valuation_snapshot ADD COLUMN price_pnl_idr TEXT;
ALTER TABLE valuation_snapshot ADD COLUMN fx_pnl_idr TEXT;
