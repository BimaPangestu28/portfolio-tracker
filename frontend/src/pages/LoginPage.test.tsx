import { render, screen } from "@testing-library/react";
import { AuthProvider } from "../auth/AuthContext";
import { ThemeProvider } from "@/components/theme-provider";
import LoginPage from "./LoginPage";

afterEach(() => localStorage.clear());

function setup() {
  return render(
    <ThemeProvider>
      <AuthProvider>
        <LoginPage />
      </AuthProvider>
    </ThemeProvider>,
  );
}

test("renders a single master-password field and submit", () => {
  setup();
  expect(screen.getByLabelText(/sandi|password/i)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /masuk|buka|login/i })).toBeInTheDocument();
});

test("has no demo button", () => {
  setup();
  expect(screen.queryByText(/demo/i)).not.toBeInTheDocument();
});

test("shows catalystlabs.id copyright in brand aside", () => {
  setup();
  expect(screen.getByText(/catalystlabs\.id/i)).toBeInTheDocument();
});
