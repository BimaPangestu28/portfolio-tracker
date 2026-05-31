import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { z } from "zod";
import { api } from "./client";
import {
  AccountSchema, CategorySchema, InstrumentSchema, TransactionSchema,
  PortfolioSummarySchema, SnapshotSchema,
  type Account, type Category, type Instrument, type Transaction,
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
  useInvalidatingMutation((b: Omit<Instrument, "id">) => api.post("/instruments", InstrumentSchema, b), ["instruments"]);
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
export const useDeleteInstrument = () => useInvalidatingMutation((id: number) => api.del(`/instruments/${id}`), ["instruments"]);
export const useDeleteTransaction = () => useInvalidatingMutation((id: number) => api.del(`/transactions/${id}`), ["transactions", "summary"]);

export type { Account, Category, Instrument, Transaction };
