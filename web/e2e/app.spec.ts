// Framework smoke test: works for any app, including after `just remove-example`.
// The app loads from the binary, has no console errors, and unknown URLs 404.
import { expect, test } from "@playwright/test";

test("home page loads without errors", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));

  await page.goto("/");
  await expect(page.locator("main h1")).toBeVisible();
  expect(errors).toEqual([]);
});

test("unknown URL shows the not-found page", async ({ page }) => {
  await page.goto("/this/does/not/exist");
  await expect(page.getByRole("heading", { name: "Not found" })).toBeVisible();
});

test("api health", async ({ request }) => {
  expect((await request.get("/readyz")).ok()).toBe(true);
});
