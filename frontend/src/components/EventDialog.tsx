import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Dialog } from "./Dialog";
import { useCreateEvent, useUpdateEvent } from "../api/hooks";
import type { EventItem } from "../api/schemas";
import { wibDateTimeToUtcZ, wibDayKey, formatWibTime } from "../lib/wib";

interface EventDialogProps {
  open: boolean;
  onClose: () => void;
  /** Edit mode when provided; create mode otherwise. */
  event?: EventItem | null;
  /** WIB day pre-selected for a new event (create mode), "YYYY-MM-DD". */
  defaultDay?: string;
}

const blankForm = (defaultDay?: string) => ({
  title: "",
  date: defaultDay ?? new Date().toISOString().slice(0, 10),
  time: "09:00",
  location: "",
  notes: "",
});

export function EventDialog({ open, onClose, event, defaultDay }: EventDialogProps) {
  const create = useCreateEvent();
  const update = useUpdateEvent();
  const isEdit = !!event;

  const [form, setForm] = useState(() => blankForm(defaultDay));

  useEffect(() => {
    if (!open) return;
    if (event) {
      setForm({
        title: event.title,
        date: wibDayKey(event.start_at),
        time: formatWibTime(event.start_at),
        location: event.location ?? "",
        notes: event.notes ?? "",
      });
    } else {
      setForm(blankForm(defaultDay));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, event, defaultDay]);

  const set =
    (k: keyof typeof form) =>
    (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) =>
      setForm((prev) => ({ ...prev, [k]: e.target.value }));

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!form.title.trim()) {
      toast.error("Judul wajib diisi");
      return;
    }
    const body = {
      title: form.title.trim(),
      start_at: wibDateTimeToUtcZ(form.date, form.time),
      location: form.location.trim() || null,
      notes: form.notes.trim() || null,
    };
    const opts = {
      onSuccess: () => {
        toast.success(isEdit ? "Agenda diperbarui" : "Agenda ditambahkan");
        onClose();
      },
      onError: (err: unknown) => toast.error((err as Error).message),
    };
    if (isEdit && event) update.mutate({ id: event.id, patch: body }, opts);
    else create.mutate(body, opts);
  };

  const isPending = create.isPending || update.isPending;

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={isEdit ? "Edit Agenda" : "Tambah Agenda"}
      sub="Acara di kalender pribadimu"
      footer={
        <>
          <button type="button" className="btn btn-outline" onClick={onClose}>
            Batal
          </button>
          <button
            type="submit"
            form="event-form"
            className="btn btn-primary"
            disabled={isPending}
            aria-label={isPending ? "Menyimpan agenda" : isEdit ? "Simpan perubahan" : "Tambah agenda"}
          >
            {isPending ? "Menyimpan…" : isEdit ? "Simpan" : "Tambah"}
          </button>
        </>
      }
    >
      <form id="event-form" onSubmit={submit}>
        <label className="field">
          <span className="field-label">Judul</span>
          <input
            className="input"
            value={form.title}
            onChange={set("title")}
            autoFocus
            aria-label="Judul agenda"
          />
        </label>
        <div className="grid" style={{ gridTemplateColumns: "1fr auto", gap: 12 }}>
          <label className="field">
            <span className="field-label">Tanggal</span>
            <input
              type="date"
              className="input"
              value={form.date}
              onChange={set("date")}
              aria-label="Tanggal agenda"
            />
          </label>
          <label className="field" style={{ minWidth: 120 }}>
            <span className="field-label">Jam (WIB)</span>
            <input
              type="time"
              className="input"
              value={form.time}
              onChange={set("time")}
              aria-label="Jam agenda dalam WIB"
            />
          </label>
        </div>
        <label className="field">
          <span className="field-label">Lokasi (opsional)</span>
          <input
            className="input"
            value={form.location}
            onChange={set("location")}
            aria-label="Lokasi agenda"
          />
        </label>
        <label className="field">
          <span className="field-label">Catatan (opsional)</span>
          <textarea
            className="input"
            rows={2}
            value={form.notes}
            onChange={set("notes")}
            aria-label="Catatan agenda"
          />
        </label>
        {(create.error || update.error) && (
          <div className="t-sm loss">
            {((create.error || update.error) as Error).message}
          </div>
        )}
      </form>
    </Dialog>
  );
}
