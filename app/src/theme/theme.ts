export type Theme = "dark" | "light";

export const THEME_STORAGE_KEY = "jingwei.workbench.theme";

interface ThemeStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export function isTheme(value: unknown): value is Theme {
  return value === "dark" || value === "light";
}

export function resolveTheme(storedTheme: string | null, prefersDark: boolean): Theme {
  if (isTheme(storedTheme)) return storedTheme;
  return prefersDark ? "dark" : "light";
}

export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement): void {
  root.dataset.theme = theme;
  root.style.colorScheme = theme;
}

export function initializeTheme(
  storage: ThemeStorage = window.localStorage,
  prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches,
  root: HTMLElement = document.documentElement,
): Theme {
  const theme = resolveTheme(storage.getItem(THEME_STORAGE_KEY), prefersDark);
  applyTheme(theme, root);
  return theme;
}

export function persistTheme(theme: Theme, storage: ThemeStorage = window.localStorage): void {
  storage.setItem(THEME_STORAGE_KEY, theme);
}

