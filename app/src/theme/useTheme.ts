import { useCallback, useLayoutEffect, useState } from "react";
import { applyTheme, initializeTheme, persistTheme, type Theme } from "./theme";

export function useTheme() {
  const [theme, setTheme] = useState<Theme>(() => initializeTheme());

  useLayoutEffect(() => {
    applyTheme(theme);
    persistTheme(theme);
  }, [theme]);

  const toggleTheme = useCallback(() => {
    setTheme((current) => current === "dark" ? "light" : "dark");
  }, []);

  return { theme, toggleTheme };
}

