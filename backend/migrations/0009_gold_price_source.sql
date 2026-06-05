-- Gold held in grams was priced manually (no quotes ever stored); switch it
-- to the derived IDR-per-gram source (Yahoo GC=F × USD/IDR) introduced
-- alongside this migration so it refreshes hourly like everything else.
UPDATE instrument
SET price_source = 'gold:idr_gram'
WHERE price_source = 'manual' AND LOWER(symbol) = 'gold';
