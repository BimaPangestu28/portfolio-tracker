import { useState } from "react";
import { useReviewItems, useIngest, useInstruments, useAccounts } from "../api/hooks";
import { readFileAsUpload, ACCEPTED_TYPES, type UploadFileIn } from "../lib/upload";
import { ReviewRow } from "../components/ReviewRow";
import { QueryState } from "../components/QueryState";

export default function ImportPage() {
  const review = useReviewItems("pending");
  const ingest = useIngest();
  const instruments = useInstruments();
  const accounts = useAccounts();
  const [busy, setBusy] = useState(false);

  const onFiles = async (fileList: FileList | null) => {
    if (!fileList || fileList.length === 0) return;
    setBusy(true);
    try {
      const uploads: UploadFileIn[] = [];
      for (const f of Array.from(fileList)) uploads.push(await readFileAsUpload(f));
      await ingest.mutateAsync(uploads);
    } finally {
      setBusy(false);
    }
  };

  const items = review.data ?? [];

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Import</h1>

      <div className="rounded border-2 border-dashed bg-white p-6 text-center">
        <label className="cursor-pointer text-sm">
          <span className="rounded bg-blue-600 px-3 py-2 text-white">
            {busy || ingest.isPending ? "Extracting…" : "Choose screenshots / PDFs"}
          </span>
          <input
            type="file"
            multiple
            accept={ACCEPTED_TYPES.join(",")}
            className="hidden"
            disabled={busy || ingest.isPending}
            onChange={(e) => onFiles(e.target.files)}
          />
        </label>
        <p className="mt-2 text-xs text-gray-500">
          PNG/JPG/WebP/PDF. Extracted entries appear below for review — nothing is saved until you confirm.
        </p>
        {ingest.error && <p className="mt-2 text-sm text-red-600">{(ingest.error as Error).message}</p>}
      </div>

      <QueryState isLoading={review.isLoading} error={review.error}>
        {items.length === 0 ? (
          <div className="text-sm text-gray-500">No pending items. Upload a document to extract transactions.</div>
        ) : (
          <div className="overflow-x-auto rounded border bg-white">
            <table className="w-full text-sm">
              <thead className="bg-gray-50 text-left text-xs uppercase text-gray-500">
                <tr>
                  <th className="p-1">Source</th>
                  <th className="p-1">Type</th>
                  <th className="p-1">Instrument</th>
                  <th className="p-1">Account</th>
                  <th className="p-1">Qty</th>
                  <th className="p-1">Price</th>
                  <th className="p-1">Ccy</th>
                  <th className="p-1">Date</th>
                  <th className="p-1"></th>
                </tr>
              </thead>
              <tbody>
                {items.map((it) => (
                  <ReviewRow key={it.id} item={it} instruments={instruments.data ?? []} accounts={accounts.data ?? []} />
                ))}
              </tbody>
            </table>
          </div>
        )}
      </QueryState>
    </div>
  );
}
