import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import NewsQuiz from "./NewsQuiz";
import type { NewsQuiz as Q } from "../api/schemas";

const QS: Q[] = [
  { position: 0, question: "Apa rilis besar?", options: ["Go", "Rust 2.0"], answer_index: 1, explanation: "Karena Rust", article_position: 0 },
];

beforeEach(() => localStorage.clear());

test("scores the quiz after submit and reveals the explanation", async () => {
  render(<NewsQuiz questions={QS} date="2026-06-16" />);
  await userEvent.click(screen.getByLabelText("Rust 2.0"));
  await userEvent.click(screen.getByRole("button", { name: /selesai|cek|submit/i }));
  expect(screen.getByText(/1\s*\/\s*1/)).toBeInTheDocument();
  expect(screen.getByText(/Karena Rust/)).toBeInTheDocument();
});

test("marks a wrong answer as incorrect", async () => {
  render(<NewsQuiz questions={QS} date="2026-06-16" />);
  await userEvent.click(screen.getByLabelText("Go"));
  await userEvent.click(screen.getByRole("button", { name: /selesai|cek|submit/i }));
  expect(screen.getByText(/0\s*\/\s*1/)).toBeInTheDocument();
});

test("persists score in localStorage and restores it on remount", async () => {
  const { unmount } = render(<NewsQuiz questions={QS} date="2026-06-16" />);
  await userEvent.click(screen.getByLabelText("Rust 2.0"));
  await userEvent.click(screen.getByRole("button", { name: /selesai|cek|submit/i }));
  expect(screen.getByText(/1\s*\/\s*1/)).toBeInTheDocument();

  unmount();

  render(<NewsQuiz questions={QS} date="2026-06-16" />);
  expect(screen.getByText(/1\s*\/\s*1/)).toBeInTheDocument();
});
