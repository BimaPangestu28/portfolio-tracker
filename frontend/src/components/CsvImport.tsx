import { useState } from "react";
import { useIngestCsv } from "../api/hooks";

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

  const input = "rounded border px-2 py-1 text-sm";

  return (
    <div className="rounded border bg-white p-4 space-y-4">
      <h2 className="text-sm font-semibold">Or import a CSV</h2>

      <div className="space-y-2">
        <label className="block text-xs text-gray-500">
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
          className="w-full rounded border px-2 py-1 text-xs font-mono"
          rows={5}
          placeholder={"Paste CSV here (first line = headers)…\nDate,Side,Ticker,Qty,Price"}
          value={csvText}
          onChange={(e) => setCsvText(e.target.value)}
        />
      </div>

      {headers.length > 0 && (
        <form onSubmit={handleSubmit} className="space-y-3">
          <h3 className="text-xs font-semibold text-gray-700">Map CSV columns to fields</h3>
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            {CSV_FIELDS.map((field) => (
              <div key={field} className="flex flex-col gap-0.5">
                <label className="text-xs text-gray-500">{field}</label>
                <select
                  aria-label={`Map ${field}`}
                  className={input}
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
            <label className="text-xs text-gray-500">
              entry_type constant (used when no column is mapped above)
            </label>
            <input
              aria-label="Entry type constant"
              className={`${input} w-48`}
              placeholder="e.g. buy"
              value={entryTypeConst}
              onChange={(e) => setEntryTypeConst(e.target.value)}
            />
          </div>

          <button
            type="submit"
            className="rounded bg-blue-600 px-4 py-1.5 text-sm text-white disabled:opacity-50"
            disabled={ingestCsv.isPending}
          >
            {ingestCsv.isPending ? "Importing…" : "Import CSV"}
          </button>

          {ingestCsv.error && (
            <div className="text-sm text-red-600">{(ingestCsv.error as Error).message}</div>
          )}
          {success && (
            <div className="text-sm text-green-600">
              Import complete — entries added to the review queue above.
            </div>
          )}
        </form>
      )}
    </div>
  );
}
