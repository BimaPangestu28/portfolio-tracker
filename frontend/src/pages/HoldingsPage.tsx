import { useSummary, useInstruments } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { formatIDR, formatUSD, formatPct } from "../lib/format";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";

export default function HoldingsPage() {
  const summary = useSummary();
  const instruments = useInstruments();
  const nameOf = (id: number) => instruments.data?.find((i) => i.id === id)?.symbol ?? `#${id}`;
  const positions = summary.data?.positions ?? [];

  return (
    <div className="space-y-4">
      <h1 className="text-xl font-semibold">Holdings</h1>
      <QueryState isLoading={summary.isLoading} error={summary.error}>
        <Card className="overflow-hidden">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Instrument</TableHead>
                <TableHead>Qty</TableHead>
                <TableHead>Avg cost</TableHead>
                <TableHead>Price</TableHead>
                <TableHead>Value (IDR)</TableHead>
                <TableHead>Unrealized</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {positions.map((p) => (
                <TableRow key={p.instrument_id}>
                  <TableCell className="font-medium">
                    {nameOf(p.instrument_id)}
                    {p.price_stale && (
                      <Badge variant="outline" className="ml-2 border-amber-500 text-amber-600 dark:text-amber-400" title="Price may be outdated">
                        ⚠ stale
                      </Badge>
                    )}
                  </TableCell>
                  <TableCell>{p.quantity}</TableCell>
                  <TableCell>{formatUSD(p.avg_cost)}</TableCell>
                  <TableCell>{formatUSD(p.latest_price)}</TableCell>
                  <TableCell>{formatIDR(p.market_value_idr)}</TableCell>
                  <TableCell className={cn(Number(p.unrealized_pnl) >= 0 ? "text-emerald-600 dark:text-emerald-400" : "text-red-600 dark:text-red-400")}>
                    {formatUSD(p.unrealized_pnl)} ({formatPct(((Number(p.unrealized_pnl) / (Number(p.cost_basis_total) || 1)) * 100).toString())})
                  </TableCell>
                </TableRow>
              ))}
              {positions.length === 0 && (
                <TableRow>
                  <TableCell className="text-muted-foreground" colSpan={6}>
                    No positions yet. Add transactions to see holdings.
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </Card>
      </QueryState>
    </div>
  );
}
