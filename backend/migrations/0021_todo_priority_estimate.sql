-- Fase 2: todo priority + duration estimate for day planning.
-- priority NULL is treated as 'normal' by the application.
ALTER TABLE todos ADD COLUMN priority TEXT
  CHECK (priority IN ('high', 'normal', 'low'));
ALTER TABLE todos ADD COLUMN estimate_minutes INTEGER;
