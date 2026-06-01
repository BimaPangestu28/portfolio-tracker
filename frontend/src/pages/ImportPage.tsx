import { useState } from "react";
import { useReviewItems, useIngest, useInstruments, useAccounts } from "../api/hooks";
import { readFileAsUpload, ACCEPTED_TYPES, type UploadFileIn } from "../lib/upload";
import { ReviewRow } from "../components/ReviewRow";
import { QueryState } from "../components/QueryState";
import { CsvImport } from "../components/CsvImport";
import { Card, CardContent } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

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

      <Card className="border-2 border-dashed">
        <CardContent className="p-6 text-center">
          <label className="cursor-pointer text-sm">
            <span className="rounded bg-primary px-3 py-2 text-primary-foreground">
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
          <p className="mt-2 text-xs text-muted-foreground">
            PNG/JPG/WebP/PDF. Extracted entries appear below for review — nothing is saved until you confirm.
          </p>
          {ingest.error && <p className="mt-2 text-sm text-destructive">{(ingest.error as Error).message}</p>}
        </CardContent>
      </Card>

      <CsvImport />

      <QueryState isLoading={review.isLoading} error={review.error}>
        {items.length === 0 ? (
          <div className="text-sm text-muted-foreground">No pending items. Upload a document to extract transactions.</div>
        ) : (
          <Card className="overflow-x-auto">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="p-1">Source</TableHead>
                  <TableHead className="p-1">Type</TableHead>
                  <TableHead className="p-1">Instrument</TableHead>
                  <TableHead className="p-1">Account</TableHead>
                  <TableHead className="p-1">Qty</TableHead>
                  <TableHead className="p-1">Price</TableHead>
                  <TableHead className="p-1">Ccy</TableHead>
                  <TableHead className="p-1">Date</TableHead>
                  <TableHead className="p-1" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.map((it) => (
                  <ReviewRow key={it.id} item={it} instruments={instruments.data ?? []} accounts={accounts.data ?? []} />
                ))}
              </TableBody>
            </Table>
          </Card>
        )}
      </QueryState>
    </div>
  );
}
