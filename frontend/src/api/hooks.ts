import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { z } from "zod";
import { api } from "./client";
import {
  AccountSchema, CategorySchema, InstrumentSchema, TransactionSchema,
  PortfolioSummarySchema, SnapshotSchema,
  ReviewItemSchema, IngestResultSchema,
  type Account, type Category, type Instrument, type Transaction, type ReviewItem,
} from "./schemas";

export const useSummary = () =>
  useQuery({ queryKey: ["summary"], queryFn: () => api.get("/portfolio/summary", PortfolioSummarySchema) });

export const useHistory = () =>
  useQuery({ queryKey: ["history"], queryFn: () => api.get("/portfolio/history", z.array(SnapshotSchema)) });

export const useAccounts = () =>
  useQuery({ queryKey: ["accounts"], queryFn: () => api.get("/accounts", z.array(AccountSchema)) });

export const useCategories = () =>
  useQuery({ queryKey: ["categories"], queryFn: () => api.get("/categories", z.array(CategorySchema)) });

export const useInstruments = () =>
  useQuery({ queryKey: ["instruments"], queryFn: () => api.get("/instruments", z.array(InstrumentSchema)) });

export const useTransactions = () =>
  useQuery({ queryKey: ["transactions"], queryFn: () => api.get("/transactions", z.array(TransactionSchema)) });

function useInvalidatingMutation<TInput>(fn: (input: TInput) => Promise<unknown>, keys: string[]) {
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
export const useCreateInstrument = () =>
  useInvalidatingMutation((b: Omit<Instrument, "id">) => api.post("/instruments", InstrumentSchema, b), ["instruments", "summary"]);
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

export interface UploadFileIn { filename: string; media_type: string; data_base64: string }

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
