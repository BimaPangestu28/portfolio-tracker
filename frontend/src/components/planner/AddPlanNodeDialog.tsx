import { useEffect, useMemo, useState } from "react";
import ReactDOM from "react-dom";
import { Check, X } from "lucide-react";
import { toast } from "sonner";
import {
  useCreatePlanNode, useCreateCategory, useCategories, useInstruments, usePlanNodes,
  type NewPlanNode, type PlanNodeAllocation,
} from "../../api/hooks";
import { NumberInput } from "@/components/ui/NumberInput";
import { boundCategoryIds, boundInstrumentIds } from "../../lib/plan-tree";

interface Props {
  open: boolean;
  parent: PlanNodeAllocation | null;
  onClose: () => void;
}

// Root nodes bind an asset-class category; child nodes bind an instrument or are a group.
type RootBind = "category";
type ChildBind = "instrument" | "group";

export function AddPlanNodeDialog({ open, parent, onClose }: Props) {
  const isRoot = parent === null;
  const createNode = useCreatePlanNode();
  const createCategory = useCreateCategory();
  const cats = useCategories();
  const instruments = useInstruments();
  const rawNodes = usePlanNodes();

  const [childKind, setChildKind] = useState<ChildBind>("instrument");
  const [categoryId, setCategoryId] = useState("");
  const [newCategoryName, setNewCategoryName] = useState("");
  const [instrumentId, setInstrumentId] = useState("");
  const [groupName, setGroupName] = useState("");
  const [target, setTarget] = useState("");
  const [tolerance, setTolerance] = useState("");

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  // Reset fields whenever the dialog (re)opens for a different parent.
  useEffect(() => {
    if (open) {
      setChildKind("instrument");
      setCategoryId(""); setNewCategoryName(""); setInstrumentId(""); setGroupName("");
      setTarget(""); setTolerance("");
    }
  }, [open, parent]);

  const usedCats = useMemo(() => boundCategoryIds(rawNodes.data ?? []), [rawNodes.data]);
  const usedInstruments = useMemo(() => boundInstrumentIds(rawNodes.data ?? []), [rawNodes.data]);
  const availableCats = (cats.data ?? []).filter((c) => !usedCats.has(c.id));
  const availableInstruments = (instruments.data ?? []).filter((i) => !usedInstruments.has(i.id));

  if (!open) return null;

  const bindKind: RootBind | ChildBind = isRoot ? "category" : childKind;

  const submit = (e: React.FormEvent) => {
    e.preventDefault();

    const afterCreate = (payload: NewPlanNode) =>
      createNode.mutate(payload, {
        onSuccess: () => { toast.success(`"${payload.name}" ditambahkan`); onClose(); },
        onError: (err) => toast.error((err as Error).message),
      });

    const base = {
      parent_id: isRoot ? null : parent!.id,
      target_pct: target || "0",
      tolerance_band_pct: tolerance || null,
      sort_order: null,
      color: null,
    };

    if (bindKind === "category") {
      if (newCategoryName.trim()) {
        // Create the category first, then a root node bound to it.
        createCategory.mutate(
          { name: newCategoryName.trim(), target_pct: "0", tolerance_band_pct: null, color: null },
          {
            onSuccess: (cat) => afterCreate({ ...base, name: cat.name, bind_kind: "category", category_id: cat.id }),
            onError: (err) => toast.error((err as Error).message),
          },
        );
        return;
      }
      const id = Number(categoryId);
      const cat = (cats.data ?? []).find((c) => c.id === id);
      if (!cat) { toast.error("Pilih kategori dulu"); return; }
      afterCreate({ ...base, name: cat.name, bind_kind: "category", category_id: cat.id });
      return;
    }

    if (bindKind === "instrument") {
      const id = Number(instrumentId);
      const ins = (instruments.data ?? []).find((i) => i.id === id);
      if (!ins) { toast.error("Pilih instrumen dulu"); return; }
      afterCreate({ ...base, name: ins.symbol, bind_kind: "instrument", instrument_id: ins.id });
      return;
    }

    // group
    if (!groupName.trim()) { toast.error("Isi nama grup dulu"); return; }
    afterCreate({ ...base, name: groupName.trim(), bind_kind: "group" });
  };

  return ReactDOM.createPortal(
    <div className="dialog-scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }} role="presentation">
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="add-node-title">
        <div className="dialog-head">
          <div>
            <div className="t-h2" id="add-node-title">
              {isRoot ? "Tambah Kelas Aset" : `Tambah di bawah ${parent!.name}`}
            </div>
            <div className="card-sub" style={{ marginTop: 3 }}>
              {isRoot ? "Node akar terikat ke kategori aset" : "Pecah jadi instrumen atau sub-grup"}
            </div>
          </div>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="Tutup dialog" style={{ width: 32, height: 32 }}>
            <X size={18} />
          </button>
        </div>

        <div className="dialog-body">
          <form id="add-node-form" onSubmit={submit}>
            {!isRoot && (
              <label className="field">
                <span className="field-label">Jenis</span>
                <select className="input" aria-label="Jenis node" value={childKind} onChange={(e) => setChildKind(e.target.value as ChildBind)}>
                  <option value="instrument">Instrumen</option>
                  <option value="group">Sub-grup</option>
                </select>
              </label>
            )}

            {bindKind === "category" && (
              <>
                <label className="field">
                  <span className="field-label">Kategori</span>
                  <select className="input" aria-label="Pilih kategori" value={categoryId} onChange={(e) => { setCategoryId(e.target.value); setNewCategoryName(""); }}>
                    <option value="">— pilih kategori —</option>
                    {availableCats.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
                  </select>
                </label>
                <label className="field">
                  <span className="field-label">…atau buat kategori baru</span>
                  <input className="input" placeholder="mis. Properti" aria-label="Nama kategori baru" value={newCategoryName} onChange={(e) => { setNewCategoryName(e.target.value); setCategoryId(""); }} />
                </label>
              </>
            )}

            {bindKind === "instrument" && (
              <label className="field">
                <span className="field-label">Instrumen</span>
                <select className="input" aria-label="Pilih instrumen" value={instrumentId} onChange={(e) => setInstrumentId(e.target.value)}>
                  <option value="">— pilih instrumen —</option>
                  {availableInstruments.map((i) => <option key={i.id} value={i.id}>{i.symbol} — {i.name}</option>)}
                </select>
              </label>
            )}

            {bindKind === "group" && (
              <label className="field">
                <span className="field-label">Nama grup</span>
                <input className="input" placeholder="mis. Perbankan" aria-label="Nama grup" value={groupName} onChange={(e) => setGroupName(e.target.value)} />
              </label>
            )}

            <div className="grid form-stack" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
              <label className="field">
                <span className="field-label">Target %</span>
                <NumberInput className="input" placeholder="0" aria-label="Target persen" value={target} onChange={setTarget} required />
              </label>
              <label className="field">
                <span className="field-label">Toleransi ± %</span>
                <NumberInput className="input" placeholder="5" aria-label="Toleransi persen" value={tolerance} onChange={setTolerance} />
              </label>
            </div>
          </form>
        </div>

        <div className="dialog-foot">
          <button type="button" className="btn btn-outline" onClick={onClose}>Batal</button>
          <button type="button" className="btn btn-primary" onClick={submit as unknown as React.MouseEventHandler} disabled={createNode.isPending} aria-label="Simpan node">
            <Check size={16} /> Simpan
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
