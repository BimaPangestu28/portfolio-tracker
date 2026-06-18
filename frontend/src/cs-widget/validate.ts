import type { Lead } from "./api";

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/** Returns an error message (Bahasa) or null when the lead is valid. */
export function validateLead(lead: Lead): string | null {
  if (!lead.name.trim()) return "Mohon isi nama kamu.";
  const hasEmail = lead.email.trim().length > 0;
  const hasPhone = lead.phone.trim().length > 0;
  if (!hasEmail && !hasPhone) return "Mohon isi email atau nomor HP supaya kami bisa menghubungi kamu.";
  if (hasEmail && !EMAIL_RE.test(lead.email.trim())) return "Format email belum benar.";
  return null;
}
