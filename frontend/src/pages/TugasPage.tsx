import { useState } from "react";
import { TodoTab } from "../components/tugas/TodoTab";

type Tab = "todo" | "reminder" | "inbox";

export default function TugasPage() {
  const [tab, setTab] = useState<Tab>("todo");

  return (
    <div className="flex col gap-5">
      <div>
        <h1 className="t-h1">Tugas</h1>
        <div className="t-sm t-muted">Kelola todo, reminder, dan inbox</div>
      </div>
      <div className="ptabs">
        <button className={`ptab${tab === "todo" ? " active" : ""}`} onClick={() => setTab("todo")}>Todo</button>
        <button className={`ptab${tab === "reminder" ? " active" : ""}`} onClick={() => setTab("reminder")}>Reminder</button>
        <button className={`ptab${tab === "inbox" ? " active" : ""}`} onClick={() => setTab("inbox")}>Inbox</button>
      </div>
      <div className="card card-pad" style={{ padding: 18 }}>
        {tab === "todo" && <TodoTab />}
        {tab === "reminder" && <p className="text-sm text-muted-foreground">Segera.</p>}
        {tab === "inbox" && <p className="text-sm text-muted-foreground">Segera.</p>}
      </div>
    </div>
  );
}
