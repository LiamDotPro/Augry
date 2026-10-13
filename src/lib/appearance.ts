import { useEffect, useState } from "react";
import { call } from "../ipc/client";
import type { ThemePreference } from "../ipc/types";
import { IS_MAC } from "./hotkeys";
import { useWindowFrame } from "./windowFrame";
import { terminalRegistry } from "../terminals/registry";
import type { ResolvedTheme } from "../terminals/theme";

const LIGHT_QUERY = "(prefers-color-scheme: light)";

/**
 * "system" becomes light or dark from the OS; the others are taken as chosen.
 * Glass needs the macOS window material, so anywhere else it falls back to
 * dark. So does a fullscreen window: it sits in a Space of its own with only
 * the wallpaper behind it, and the glass turned into a flat wash next to the
 * black band macOS draws around the notch.
 */
export function resolveTheme(
  preference: ThemePreference,
  systemPrefersLight: boolean,
  mac: boolean = IS_MAC,
  fullscreen = false,
): ResolvedTheme {
  if (preference === "system") return systemPrefersLight ? "light" : "dark";
  if (preference === "glass" && (!mac || fullscreen)) return "dark";
  return preference;
}

function useSystemPrefersLight(): boolean {
  const [light, setLight] = useState(() => window.matchMedia?.(LIGHT_QUERY).matches ?? false);
  useEffect(() => {
    const query = window.matchMedia?.(LIGHT_QUERY);
    if (!query) return;
    const onChange = () => setLight(query.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);
  return light;
}

/** The colours Settings can override; null follows the theme. */
export interface AppearanceColors {
  focus: string | null;
  finished: string | null;
}

/** `#rrggbb` as the space separated channels the tokens use ("141 220 255"), or null. */
export function hexToChannels(hex: string | null): string | null {
  const match = hex ? /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex.trim()) : null;
  if (!match) return null;
  return match.slice(1).map((pair) => parseInt(pair, 16)).join(" ");
}

/** Sets a channel token inline on <html>, or removes it so the theme's value applies. */
function setChannels(name: string, hex: string | null): void {
  const channels = hexToChannels(hex);
  const style = document.documentElement.style;
  if (channels) style.setProperty(name, channels);
  else style.removeProperty(name);
}

/**
 * Puts the theme and layout on <html> (data-theme, data-layout), where the
 * tokens in styles/tokens.css pick them up, along with any custom focus and
 * finished colours, repaints the terminals and, on macOS, turns the native
 * window glass on for the Glass theme. While
 * the preference is "system" it follows the OS as it changes. An inactive
 * caller leaves <html> alone, so another one (the settings preview) can drive
 * it; going active again reapplies its own values.
 */
export function useAppearance(
  preference: ThemePreference,
  compact: boolean,
  colors: AppearanceColors,
  active = true,
): void {
  const systemPrefersLight = useSystemPrefersLight();
  const fullscreen = useWindowFrame() === "fullscreen";
  const theme = resolveTheme(preference, systemPrefersLight, IS_MAC, fullscreen);

  useEffect(() => {
    if (!active) return;
    document.documentElement.dataset.theme = theme;
    terminalRegistry.setTheme(theme);
  }, [theme, active]);

  const glass = theme === "glass";
  useEffect(() => {
    if (!active || !IS_MAC) return;
    call("set_window_glass", { enabled: glass }).catch((error) =>
      console.error("augry: window glass unavailable", error),
    );
  }, [glass, active]);

  useEffect(() => {
    if (!active) return;
    document.documentElement.dataset.layout = compact ? "compact" : "comfortable";
  }, [compact, active]);

  useEffect(() => {
    if (!active) return;
    setChannels("--focus-rgb", colors.focus);
    setChannels("--finished-rgb", colors.finished);
  }, [colors.focus, colors.finished, active]);
}
