/**
 * LoginPage tests — Phase 5E auth screens.
 *
 * Tests cover:
 *  - First-run setup shows "Buat sandi master"
 *  - Existing password shows login form ("Selamat datang kembali")
 *  - Demo button bypasses password gate
 */

import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ThemeProvider } from "@/components/theme-provider";
import { AuthProvider } from "@/auth/AuthContext";
import LoginPage from "./LoginPage";

// Mock SubtleCrypto.digest for jsdom (not available in test environment)
const mockDigest = vi.fn().mockImplementation(async (_algo: string, data: BufferSource) => {
  // Return deterministic mock hash based on input length
  const arr = new Uint8Array(data instanceof ArrayBuffer ? data : (data as Uint8Array));
  const buf = new ArrayBuffer(32);
  const view = new Uint8Array(buf);
  view[0] = arr.length & 0xff;
  return buf;
});

Object.defineProperty(global, "crypto", {
  value: { subtle: { digest: mockDigest } },
  writable: true,
});

function renderLoginPage() {
  return render(
    <ThemeProvider defaultTheme="dark" storageKey="test-theme">
      <AuthProvider>
        <LoginPage />
      </AuthProvider>
    </ThemeProvider>,
  );
}

afterEach(() => {
  localStorage.removeItem("pt-auth-hash");
  localStorage.removeItem("pt-auth-session");
  localStorage.removeItem("pt-auth-demo");
  vi.clearAllMocks();
});

// ── First-run setup ──────────────────────────────────────────────────────────

test("first-run: shows 'Buat sandi master' when no password stored", () => {
  renderLoginPage();
  expect(screen.getByText("Buat sandi master")).toBeInTheDocument();
});

test("first-run: shows setup subtitle copy", () => {
  renderLoginPage();
  expect(screen.getByText(/Sandi ini membuka akses/i)).toBeInTheDocument();
});

test("first-run: has sandi baru and konfirmasi fields", () => {
  renderLoginPage();
  expect(screen.getByLabelText("Sandi baru")).toBeInTheDocument();
  expect(screen.getByLabelText("Konfirmasi sandi")).toBeInTheDocument();
});

test("first-run: shows error if passwords don't match", async () => {
  renderLoginPage();
  fireEvent.change(screen.getByLabelText("Sandi baru"), { target: { value: "abc123" } });
  fireEvent.change(screen.getByLabelText("Konfirmasi sandi"), { target: { value: "xyz999" } });
  fireEvent.click(screen.getByRole("button", { name: /Buat.*masuk/i }));
  await waitFor(() => {
    expect(screen.getByRole("alert")).toHaveTextContent("Konfirmasi sandi tidak cocok");
  });
});

test("first-run: shows error if password too short", async () => {
  renderLoginPage();
  fireEvent.change(screen.getByLabelText("Sandi baru"), { target: { value: "abc" } });
  fireEvent.change(screen.getByLabelText("Konfirmasi sandi"), { target: { value: "abc" } });
  fireEvent.click(screen.getByRole("button", { name: /Buat.*masuk/i }));
  await waitFor(() => {
    expect(screen.getByRole("alert")).toHaveTextContent("minimal 6 karakter");
  });
});

test("first-run: shows demo button", () => {
  renderLoginPage();
  expect(
    screen.getByRole("button", { name: /Masuk dengan data demo/i }),
  ).toBeInTheDocument();
});

// ── Login form (password exists) ─────────────────────────────────────────────

test("login: shows 'Selamat datang kembali' when password is stored", () => {
  localStorage.setItem("pt-auth-hash", "fakehash1234");
  renderLoginPage();
  expect(screen.getByText("Selamat datang kembali")).toBeInTheDocument();
});

test("login: shows sandi master field", () => {
  localStorage.setItem("pt-auth-hash", "fakehash1234");
  renderLoginPage();
  expect(screen.getByLabelText("Sandi master")).toBeInTheDocument();
});

test("login: shows Lupa sandi? link", () => {
  localStorage.setItem("pt-auth-hash", "fakehash1234");
  renderLoginPage();
  expect(screen.getByText("Lupa sandi?")).toBeInTheDocument();
});

test("login: shows demo button", () => {
  localStorage.setItem("pt-auth-hash", "fakehash1234");
  renderLoginPage();
  expect(
    screen.getByRole("button", { name: /Masuk dengan data demo/i }),
  ).toBeInTheDocument();
});

// ── Demo shortcut ────────────────────────────────────────────────────────────

test("demo button sets pt-auth-demo in localStorage", () => {
  renderLoginPage();
  fireEvent.click(screen.getByRole("button", { name: /Masuk dengan data demo/i }));
  expect(localStorage.getItem("pt-auth-demo")).toBe("1");
});

// ── Brand / layout ───────────────────────────────────────────────────────────

test("shows catalystlabs.id copyright in brand aside", () => {
  renderLoginPage();
  expect(screen.getByText(/catalystlabs\.id/i)).toBeInTheDocument();
});

test("shows Portfolio brand name", () => {
  renderLoginPage();
  const els = screen.getAllByText("Portfolio");
  expect(els.length).toBeGreaterThanOrEqual(1);
});
