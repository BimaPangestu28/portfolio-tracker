import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { z } from "zod";
import { api } from "./client";
import type { UploadFileIn } from "../lib/upload";
import {
  AccountSchema, CategorySchema, InstrumentSchema, TransactionSchema,
  PortfolioSummarySchema, SnapshotSchema,
  ReviewItemSchema, IngestResultSchema,
  CashflowCategorySchema, CashflowSchema, MonthSummarySchema,
  ConnectorSchema, SyncReportSchema,
  ChatMessageSchema, ChatReplySchema,
  InsightsSchema, GoalSchema,
  PerformanceSchema,
  WhatsappStatusSchema,
  TelegramStatusSchema, TelegramLinkCodeSchema,
  MoverSchema, BenchmarkRowSchema, EventSchema,
  TodoSchema, ReminderSchema, InboxItemSchema,
  type Account, type Category, type Instrument, type Transaction, type ReviewItem,
  type CashflowCategory, type Cashflow, type Connector, type Goal,
} from "./schemas";

export const useSummary = () =>
  useQuery({ queryKey: ["summary"], queryFn: () => api.get("/portfolio/summary", PortfolioSummarySchema) });

export const useHistory = () =>
  useQuery({ queryKey: ["history"], queryFn: () => api.get("/portfolio/history", z.array(SnapshotSchema)) });

export const usePerformance = (base: "idr" | "usd", period: string) =>
  useQuery({
    queryKey: ["performance", base, period],
    queryFn: () => api.get(`/portfolio/performance?base=${base}&period=${period}`, PerformanceSchema),
  });

export const useAccounts = () =>
  useQuery({ queryKey: ["accounts"], queryFn: () => api.get("/accounts", z.array(AccountSchema)) });

export const useCategories = () =>
  useQuery({ queryKey: ["categories"], queryFn: () => api.get("/categories", z.array(CategorySchema)) });

export const useInstruments = () =>
  useQuery({ queryKey: ["instruments"], queryFn: () => api.get("/instruments", z.array(InstrumentSchema)) });

export const useTransactions = () =>
  useQuery({ queryKey: ["transactions"], queryFn: () => api.get("/transactions", z.array(TransactionSchema)) });

function useInvalidatingMutation<TInput, TData = unknown>(fn: (input: TInput) => Promise<TData>, keys: string[]) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: () => { keys.forEach((k) => qc.invalidateQueries({ queryKey: [k] })); },
  });
}

export const useCreateAccount = () =>
  useInvalidatingMutation((b: Omit<Account, "id" | "created_at">) => api.post("/accounts", AccountSchema, b), ["accounts"]);
export const useCreateCategory = () =>
  useInvalidatingMutation((b: Omit<Category, "id" | "sort_order">) => api.post("/categories", CategorySchema, b), ["categories", "summary"]);
export const useUpdateCategory = () =>
  useInvalidatingMutation(
    (args: { id: number; patch: Partial<Pick<Category, "name" | "target_pct" | "tolerance_band_pct" | "sort_order" | "color">> }) =>
      api.patch(`/categories/${args.id}`, CategorySchema, args.patch),
    ["categories", "summary"],
  );
export const useCreateInstrument = () =>
  useInvalidatingMutation((b: Omit<Instrument, "id">) => api.post("/instruments", InstrumentSchema, b), ["instruments", "summary"]);
export const useUpdateInstrument = () =>
  useInvalidatingMutation(
    (args: { id: number; patch: Partial<Pick<Instrument, "name" | "instrument_type" | "price_source" | "decimals" | "category_id">> }) =>
      api.patch(`/instruments/${args.id}`, InstrumentSchema, args.patch),
    ["instruments", "summary"],
  );
export const useCreateTransaction = () =>
  useInvalidatingMutation((b: Record<string, unknown>) => api.post("/transactions", TransactionSchema, b), ["transactions", "summary"]);
export const useManualPrice = () =>
  useInvalidatingMutation((b: { instrument_id: number; price: string; currency: string; as_of: string }) => api.post("/prices/manual", z.unknown(), b), ["summary"]);
export const useManualFx = () =>
  useInvalidatingMutation((b: { base: string; quote: string; rate: string; as_of: string }) => api.post("/fx/manual", z.unknown(), b), ["summary"]);
export const useRefreshPrices = () =>
  useInvalidatingMutation((_: void) => api.post("/prices/refresh", z.unknown(), {}), ["summary"]);

export const useDeleteAccount = () => useInvalidatingMutation((id: number) => api.del(`/accounts/${id}`), ["accounts"]);
export const useDeleteCategory = () => useInvalidatingMutation((id: number) => api.del(`/categories/${id}`), ["categories", "summary"]);
export const useDeleteInstrument = () => useInvalidatingMutation((id: number) => api.del(`/instruments/${id}`), ["instruments", "summary"]);
export const useDeleteTransaction = () => useInvalidatingMutation((id: number) => api.del(`/transactions/${id}`), ["transactions", "summary"]);

export type { Account, Category, Instrument, Transaction };

export const useReviewItems = (status = "pending") =>
  useQuery({ queryKey: ["review", status], queryFn: () => api.get(`/ingest/review?status=${status}`, z.array(ReviewItemSchema)) });

export const useIngest = () =>
  useInvalidatingMutation((files: UploadFileIn[]) => api.post("/ingest", IngestResultSchema, { files }), ["review"]);

export const usePatchReview = () =>
  useInvalidatingMutation((args: { id: number; payload_json: unknown }) =>
    api.patch(`/ingest/review/${args.id}`, ReviewItemSchema, { payload_json: args.payload_json }), ["review"]);

export const useConfirmReview = () =>
  useInvalidatingMutation((args: { id: number; payload: Record<string, unknown> }) =>
    api.post(`/ingest/review/${args.id}/confirm`, z.object({ created_txn_id: z.number() }), args.payload), ["review", "summary", "transactions"]);

export const useRejectReview = () =>
  useInvalidatingMutation((id: number) => api.post(`/ingest/review/${id}/reject`, z.unknown(), {}), ["review"]);

export type { ReviewItem };

// ── Cashflow + Budget hooks ────────────────────────────────────────────────

export const useCashflow = () =>
  useQuery({ queryKey: ["cashflow"], queryFn: () => api.get("/cashflow", z.array(CashflowSchema)) });

export const useCashflowCategories = () =>
  useQuery({ queryKey: ["cashflow-categories"], queryFn: () => api.get("/cashflow/categories", z.array(CashflowCategorySchema)) });

export const useMonthSummary = (month: string) =>
  useQuery({ queryKey: ["cashflow-summary", month], queryFn: () => api.get(`/cashflow/summary?month=${month}`, MonthSummarySchema) });

export const useCreateCashflow = () =>
  useInvalidatingMutation(
    (b: Omit<Cashflow, "id" | "created_at">) => api.post("/cashflow", CashflowSchema, b),
    ["cashflow", "cashflow-summary"],
  );

export const useDeleteCashflow = () =>
  useInvalidatingMutation((id: number) => api.del(`/cashflow/${id}`), ["cashflow", "cashflow-summary"]);

export const useCreateCashflowCategory = () =>
  useInvalidatingMutation(
    (b: Omit<CashflowCategory, "id">) => api.post("/cashflow/categories", CashflowCategorySchema, b),
    ["cashflow-categories"],
  );

export const useDeleteCashflowCategory = () =>
  useInvalidatingMutation((id: number) => api.del(`/cashflow/categories/${id}`), ["cashflow-categories"]);

export const useIngestCsv = () =>
  useInvalidatingMutation(
    (args: { filename: string; csv_text: string; mapping: Record<string, string>; entry_type_const?: string }) =>
      api.post("/ingest/csv", IngestResultSchema, args),
    ["review"],
  );

export type { CashflowCategory, Cashflow };

// ── Connector hooks ────────────────────────────────────────────────────────

export const useConnectors = () =>
  useQuery({ queryKey: ["connectors"], queryFn: () => api.get("/connectors", z.array(ConnectorSchema)) });

export const useCreateConnector = () =>
  useInvalidatingMutation(
    (b: Omit<Connector, "id" | "cursor" | "last_synced_at" | "enabled" | "created_at">) =>
      api.post("/connectors", ConnectorSchema, b),
    ["connectors"],
  );

export const useDeleteConnector = () =>
  useInvalidatingMutation((id: number) => api.del(`/connectors/${id}`), ["connectors"]);

export const useSyncConnector = () =>
  useInvalidatingMutation(
    (id: number) => api.post(`/connectors/${id}/sync`, SyncReportSchema, {}),
    ["connectors", "summary", "transactions", "review"],
  );

export type { Connector };

// ── Phase 5A: Insights + Goals hooks ──────────────────────────────────────

export const useInsights = () =>
  useQuery({
    queryKey: ["insights"],
    queryFn: () => api.get("/portfolio/insights", InsightsSchema),
  });

export const useGoals = () =>
  useQuery({ queryKey: ["goals"], queryFn: () => api.get("/goals", z.array(GoalSchema)) });

export const useCreateGoal = () =>
  useInvalidatingMutation(
    (b: Omit<Goal, "id" | "created_at">) => api.post("/goals", GoalSchema, b),
    ["goals"],
  );

export const useDeleteGoal = () =>
  useInvalidatingMutation((id: number) => api.del(`/goals/${id}`), ["goals"]);

export type { Goal };

// ── Chat hooks ─────────────────────────────────────────────────────────────

export const useChatHistory = () =>
  useQuery({ queryKey: ["chat-history"], queryFn: () => api.get("/chat/history", z.array(ChatMessageSchema)) });

export const useSendChat = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (message: string) => api.post("/chat", ChatReplySchema, { message }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["chat-history"] }); },
  });
};

// ── WhatsApp connection hooks ────────────────────────────────────────────────

export const useWhatsappStatus = () =>
  useQuery({
    queryKey: ["whatsapp-status"],
    queryFn: () => api.get("/whatsapp/status", WhatsappStatusSchema),
    refetchInterval: 2000,
  });

export const useConnectWhatsapp = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post("/whatsapp/connect", z.unknown(), {}),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["whatsapp-status"] }); },
  });
};

export const useDisconnectWhatsapp = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post("/whatsapp/disconnect", z.unknown(), {}),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["whatsapp-status"] }); },
  });
};

// ── Dashboard: movers + benchmark hooks ─────────────────────────────────────

export const useMovers = () =>
  useQuery({ queryKey: ["movers"], queryFn: () => api.get("/portfolio/movers", z.array(MoverSchema)) });

export const useBenchmark = () =>
  useQuery({ queryKey: ["benchmark"], queryFn: () => api.get("/portfolio/benchmark", z.array(BenchmarkRowSchema)) });

// ── Telegram connection hooks ────────────────────────────────────────────────

export const useTelegramStatus = () =>
  useQuery({
    queryKey: ["telegram-status"],
    queryFn: () => api.get("/telegram/status", TelegramStatusSchema),
    refetchInterval: 2000,
  });

export const useTelegramLinkCode = () =>
  useMutation({
    mutationFn: () => api.post("/telegram/link-code", TelegramLinkCodeSchema, {}),
  });

export const useUnlinkTelegram = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post("/telegram/unlink", z.unknown(), {}),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["telegram-status"] }); },
  });
};

// ── Agenda: event hooks ───────────────────────────────────────────────────────

export const useEvents = (fromZ: string, toZ: string) =>
  useQuery({
    queryKey: ["events", fromZ, toZ],
    queryFn: () =>
      api.get(`/events?from=${encodeURIComponent(fromZ)}&to=${encodeURIComponent(toZ)}`, z.array(EventSchema)),
  });

// ── Assistant: todo / reminder / inbox hooks ──────────────────────────────────

export const useTodos = () =>
  useQuery({ queryKey: ["todos"], queryFn: () => api.get("/todos", z.array(TodoSchema)) });

export const useReminders = () =>
  useQuery({ queryKey: ["reminders"], queryFn: () => api.get("/reminders", z.array(ReminderSchema)) });

export const useInbox = () =>
  useQuery({ queryKey: ["inbox"], queryFn: () => api.get("/inbox", z.array(InboxItemSchema)) });

type EventBody = { title: string; start_at: string; location?: string | null; notes?: string | null };

export const useCreateEvent = () =>
  useInvalidatingMutation((b: EventBody) => api.post("/events", EventSchema, b), ["events"]);

export const useUpdateEvent = () =>
  useInvalidatingMutation(
    (args: { id: number; patch: EventBody }) => api.patch(`/events/${args.id}`, EventSchema, args.patch),
    ["events"],
  );

export const useCancelEvent = () =>
  useInvalidatingMutation((id: number) => api.post(`/events/${id}/cancel`, z.unknown(), {}), ["events"]);

export const useCreateTodo = () =>
  useInvalidatingMutation((b: { title: string }) => api.post("/todos", TodoSchema, b), ["todos"]);

export const useCompleteTodo = () =>
  useInvalidatingMutation((id: number) => api.post(`/todos/${id}/complete`, z.unknown(), {}), ["todos"]);

export const useCancelReminder = () =>
  useInvalidatingMutation((id: number) => api.post(`/reminders/${id}/cancel`, z.unknown(), {}), ["reminders"]);

export const useResolveInbox = () =>
  useInvalidatingMutation((id: number) => api.post(`/inbox/${id}/resolve`, z.unknown(), {}), ["inbox"]);
