// Example app smoke test (deleted by `just remove-example`): one pass through
// every pattern the notes app shows. Create, background job, URL filters,
// optimistic toggle, edit, delete with redirect, not-found from the API.
import { expect, test } from "@playwright/test";

test("notes: create, count, filter, edit, delete", async ({ page }) => {
  const title = `Smoke ${Date.now()}`;
  await page.goto("/");

  // Create; the form clears and the background job fills in the word count.
  await page.getByLabel("Title").fill(title);
  await page.getByLabel("Body").fill("one two three");
  await page.getByRole("button", { name: "Add note" }).click();
  await expect(page.getByLabel("Title")).toHaveValue("");
  const row = page.locator("li", { hasText: title });
  await expect(row).toContainText("3 words", { timeout: 15_000 });

  // Search lives in the URL and survives a reload.
  await page.getByLabel("Search notes").fill(title);
  await expect(page).toHaveURL(/\?q=Smoke/);
  await page.reload();
  await expect(row).toBeVisible();

  // Toggle done, then filter on it.
  // click(), not check(): the box is controlled, so it can flash back for a frame
  // before the optimistic state renders. The assertions below wait for the result.
  await row.getByLabel("Done").click();
  await expect(row.getByLabel("Done")).toBeChecked();
  await expect(row).toHaveClass(/done/);
  await page.getByLabel("Filter").selectOption("done");
  await expect(page).toHaveURL(/status=done/);
  await expect(row).toBeVisible();

  // Open, edit, save.
  await row.getByRole("link", { name: title }).click();
  await expect(page).toHaveURL(/\/notes\/[0-9a-f-]{36}$/);
  const url = page.url();
  await page.getByLabel("Title").fill(`${title} edited`);
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Saved")).toBeVisible();

  // Delete: back to the list, and the old URL is a 404 from the API.
  await page.getByRole("button", { name: "Delete" }).click();
  await expect(page).toHaveURL(/\/$/);
  await expect(page.getByRole("heading", { name: "Notes" })).toBeVisible();
  await page.goto(url);
  await expect(page.getByRole("heading", { name: "Not found" })).toBeVisible();
});
