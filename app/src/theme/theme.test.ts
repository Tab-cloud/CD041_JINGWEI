import { beforeEach, describe, expect, it } from "vitest";
import { applyTheme, initializeTheme, resolveTheme, THEME_STORAGE_KEY } from "./theme";

describe("workbench theme", () => {
  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.style.colorScheme = "";
  });

  it("prefers a stored theme over the operating-system preference", () => {
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("falls back to the operating-system preference", () => {
    expect(resolveTheme(null, true)).toBe("dark");
    expect(resolveTheme(null, false)).toBe("light");
  });

  it("initializes the document from persistent storage", () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "light");

    expect(initializeTheme(window.localStorage, true)).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.documentElement.style.colorScheme).toBe("light");
  });

  it("applies a theme to the provided document root", () => {
    const root = document.createElement("div");
    applyTheme("dark", root);
    expect(root.dataset.theme).toBe("dark");
    expect(root.style.colorScheme).toBe("dark");
  });
});

