import { useState } from "react";
import { useIngestCsv } from "../api/hooks";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";

const CSV_FIELDS = [
  "entry_type",
  "symbol",
  "quantity",
  "price_native",
  "fee_native",
  "currency",
  "executed_at",
  "account_hint",
] as const;

type CsvField = (typeof CSV_FIELDS)[number];

const nativeSelect =
  "flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring";

export function CsvImport() {
  const [csvText, setCsvText] = useState("");
  const [mapping, setMapping] = useState<Partial<Record<CsvField, string>>>({});
  const [entryTypeConst, setEntryTypeConst] = useState("");
  const [success, setSuccess] = useState(false);
  const ingestCsv = useIngestCsv();

  const headers = csvText.split("\n")[0]
    ? csvText
        .split("\n")[0]
        .split(",")
        .map((h) => h.trim())
        .filter((h) => h.length > 0)
    : [];

  const handleFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    setCsvText(text);
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const cleanMapping: Record<string, string> = {};
    for (const [field, header] of Object.entries(mapping)) {
      if (header) cleanMapping[field] = header;
    }
    ingestCsv.mutate(
      {
        filename: "import.csv",
        csv_text: csvText,
        mapping: cleanMapping,
        entry_type_const: entryTypeConst || undefined,
      },
      {
        onSuccess: () => {
          setSuccess(true);
          setCsvText("");
          setMapping({});
          setEntryTypeConst("");
        },
      },
    );
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">Or import a CSV</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2">
          <label className="block text-xs text-muted-foreground">
            Upload a .csv file
            <input
              aria-label="CSV file"
              type="file"
              accept=".csv,text/csv"
              className="ml-2 text-sm"
              onChange={handleFile}
            />
          </label>
          <textarea
            aria-label="CSV text"
            className="w-full rounded-md border border-input bg-transparent px-3 py-2 text-xs font-mono shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            rows={5}
            placeholder={"Paste CSV here (first line = headers)…\nDate,Side,Ticker,Qty,Price"}
            value={csvText}
            onChange={(e) => setCsvText(e.target.value)}
          />
        </div>

        {headers.length > 0 && (
          <form onSubmit={handleSubmit} className="space-y-3">
            <h3 className="text-xs font-semibold text-muted-foreground">Map CSV columns to fields</h3>
            <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
              {CSV_FIELDS.map((field) => (
                <div key={field} className="flex flex-col gap-0.5">
                  <label className="text-xs text-muted-foreground">{field}</label>
                  <select
                    aria-label={`Map ${field}`}
                    className={nativeSelect}
                    value={mapping[field] ?? ""}
                    onChange={(e) =>
                      setMapping({ ...mapping, [field]: e.target.value || undefined })
                    }
                  >
                    <option value="">— skip —</option>
                    {headers.map((h) => (
                      <option key={h} value={h}>
                        {h}
                      </option>
                    ))}
                  </select>
                </div>
              ))}
            </div>

            <div className="flex flex-col gap-0.5">
              <label className="text-xs text-muted-foreground">
                entry_type constant (used when no column is mapped above)
              </label>
              <Input
                aria-label="Entry type constant"
                className="w-48"
                placeholder="e.g. buy"
                value={entryTypeConst}
                onChange={(e) => setEntryTypeConst(e.target.value)}
              />
            </div>

            <Button
              type="submit"
              disabled={ingestCsv.isPending}
            >
              {ingestCsv.isPending ? "Importing…" : "Import CSV"}
            </Button>

            {ingestCsv.error && (
              <div className="text-sm text-destructive">{(ingestCsv.error as Error).message}</div>
            )}
            {success && (
              <div className="text-sm text-emerald-600 dark:text-emerald-400">
                Import complete — entries added to the review queue above.
              </div>
            )}
          </form>
        )}
      </CardContent>
    </Card>
  );
}
