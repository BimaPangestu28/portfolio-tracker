import { useState } from "react";
import type { NewsQuiz as Q } from "../api/schemas";

export default function NewsQuiz({ questions }: { questions: Q[] }) {
  const [answers, setAnswers] = useState<Record<number, number>>({});
  const [submitted, setSubmitted] = useState(false);

  if (questions.length === 0) return null;

  const score = questions.filter((q) => answers[q.position] === q.answer_index).length;

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
        <button
          type="button"
          className="btn btn-primary btn-sm"
          style={{ marginTop: 16 }}
          onClick={() => setSubmitted(true)}
        >
          Selesai
        </button>
      ) : (
        <div style={{ marginTop: 16, display: "flex", alignItems: "center", gap: 12 }}>
          <p style={{ fontWeight: 600 }}>
            Skor: {score} / {questions.length}
          </p>
          <button
            type="button"
            className="btn btn-outline btn-sm"
            onClick={() => {
              setSubmitted(false);
              setAnswers({});
            }}
          >
            Ulangi
          </button>
        </div>
      )}
    </section>
  );
}
