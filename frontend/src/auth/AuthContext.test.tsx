import { http, HttpResponse } from "msw";
import { act, renderHook, waitFor } from "@testing-library/react";
import { server } from "../test/server";
import { AuthProvider, useAuth } from "./AuthContext";

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <AuthProvider>{children}</AuthProvider>
);

afterEach(() => localStorage.clear());

test("starts locked with no token", () => {
  const { result } = renderHook(() => useAuth(), { wrapper });
  expect(result.current.isUnlocked).toBe(false);
});

test("unlock stores token and unlocks on success", async () => {
  server.use(http.post("/api/auth/login", () => HttpResponse.json({ token: "abc" })));
  const { result } = renderHook(() => useAuth(), { wrapper });
  await act(async () => {
    const r = await result.current.unlock("pw");
    expect(r.ok).toBe(true);
  });
  await waitFor(() => expect(result.current.isUnlocked).toBe(true));
  expect(localStorage.getItem("pt-auth-token")).toBe("abc");
});

test("unlock returns error on wrong password", async () => {
  server.use(
    http.post("/api/auth/login", () =>
      HttpResponse.json({ error: "Sandi salah" }, { status: 401 }),
    ),
  );
  const { result } = renderHook(() => useAuth(), { wrapper });
  await act(async () => {
    const r = await result.current.unlock("bad");
    expect(r.ok).toBe(false);
    expect(r.error).toMatch(/salah/i);
  });
  expect(result.current.isUnlocked).toBe(false);
});

test("lock clears token", async () => {
  localStorage.setItem("pt-auth-token", "abc");
  const { result } = renderHook(() => useAuth(), { wrapper });
  expect(result.current.isUnlocked).toBe(true);
  act(() => result.current.lock());
  expect(result.current.isUnlocked).toBe(false);
  expect(localStorage.getItem("pt-auth-token")).toBeNull();
});
