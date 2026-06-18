import { useEffect, useState } from "react";
import type { NewsQuiz as Q } from "../api/schemas";

export default function NewsQuiz({ questions, date }: { questions: Q[]; date: string }) {
  const [answers, setAnswers] = useState<Record<number, number>>({});
  const [submitted, setSubmitted] = useState(false);
  const [savedScore, setSavedScore] = useState<{ score: number; total: number } | null>(null);

  const storageKey = `news-quiz-${date}`;

  useEffect(() => {
    if (!date) return;
    const raw = localStorage.getItem(storageKey);
    if (raw) {
      try {
        const parsed = JSON.parse(raw) as { score: number; total: number };
        setSavedScore(parsed);
      } catch {
        // ignore malformed data
      }
    }
  }, [storageKey, date]);

  if (questions.length === 0) return null;

  if (savedScore && !submitted) {
    return (
      <section className="card card-pad">
        <h2 className="card-title">Kuis hari ini</h2>
        <div style={{ marginTop: 16, display: "flex", alignItems: "center", gap: 12 }}>
          <p style={{ fontWeight: 600 }}>
            Skor: {savedScore.score} / {savedScore.total}
          </p>
          <button
            type="button"
            className="btn btn-outline btn-sm"
            onClick={() => {
              localStorage.removeItem(storageKey);
              setSavedScore(null);
              setAnswers({});
              setSubmitted(false);
            }}
          >
            Ulangi
          </button>
        </div>
      </section>
    );
  }

  const score = questions.filter((q) => answers[q.position] === q.answer_index).length;
  const answeredCount = Object.keys(answers).length;

  const handleSubmit = () => {
    const finalScore = questions.filter((q) => answers[q.position] === q.answer_index).length;
    const result = { score: finalScore, total: questions.length };
    localStorage.setItem(storageKey, JSON.stringify(result));
    setSavedScore(result);
    setSubmitted(true);
  };

  const handleUlangi = () => {
    localStorage.removeItem(storageKey);
    setSavedScore(null);
    setSubmitted(false);
    setAnswers({});
  };

  return (
    <section className="card card-pad">
      <h2 className="card-title">Kuis hari ini</h2>
      {questions.map((q) => {
        const picked = answers[q.position];
        return (
          <div key={q.position} className="mt-4" style={{ marginTop: 16 }}>
            <p style={{ fontWeight: 500 }}>{q.question}</p>
            {q.options.map((opt, i) => {
              const correct = submitted && i === q.answer_index;
              const wrong = submitted && picked === i && i !== q.answer_index;
              return (
                <label
                  key={i}
                  style={{
                    display: "block",
                    color: correct ? "hsl(var(--gain))" : wrong ? "hsl(var(--loss))" : undefined,
                    marginTop: 6,
                    cursor: submitted ? "default" : "pointer",
                  }}
                >
                  <input
                    type="radio"
                    name={`q-${q.position}`}
                    aria-label={opt}
                    checked={picked === i}
                    disabled={submitted}
                    onChange={() => setAnswers((a) => ({ ...a, [q.position]: i }))}
                    style={{ marginRight: 6 }}
                  />
                  {opt}
                </label>
              );
            })}
            {submitted && q.explanation && (
              <p style={{ marginTop: 6, fontSize: 13, color: "hsl(var(--muted-foreground))" }}>{q.explanation}</p>
            )}
          </div>
        );
      })}
      {!submitted ? (
        <div style={{ marginTop: 16, display: "flex", alignItems: "center", gap: 12 }}>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={handleSubmit}
          >
            Selesai
          </button>
          <span style={{ fontSize: 13, color: "hsl(var(--muted-foreground))" }}>
            Terjawab {answeredCount}/{questions.length}
          </span>
        </div>
      ) : (
        <div style={{ marginTop: 16, display: "flex", alignItems: "center", gap: 12 }}>
          <p style={{ fontWeight: 600 }}>
            Skor: {score} / {questions.length}
          </p>
          <button
            type="button"
            className="btn btn-outline btn-sm"
            onClick={handleUlangi}
          >
            Ulangi
          </button>
        </div>
      )}
    </section>
  );
}
