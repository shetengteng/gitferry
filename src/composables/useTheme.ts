import { ref } from "vue";

export type Theme = "light" | "dark";

const THEME_KEY = "gitferry-theme";

const theme = ref<Theme>("dark");

function apply() {
  document.documentElement.classList.toggle("dark", theme.value === "dark");
}

export function useTheme() {
  function init() {
    let saved: string | null = null;
    try {
      saved = localStorage.getItem(THEME_KEY);
    } catch {
      saved = null;
    }
    if (saved === "light" || saved === "dark") {
      theme.value = saved;
    } else {
      theme.value = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    }
    apply();
  }

  function toggle() {
    theme.value = theme.value === "dark" ? "light" : "dark";
    try {
      localStorage.setItem(THEME_KEY, theme.value);
    } catch {
      // 存储不可用时主题仍在本会话内生效
    }
    apply();
  }

  return { theme, init, toggle };
}
