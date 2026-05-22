import {
  createContext,
  use,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
} from "react";
import { toast } from "sonner";
import { readThemeCookie, writeThemeCookie, type ThemePreference } from "./cookie";
import { fetchMe, patchTheme } from "./api";

const PATCH_FAILURE_MESSAGE = "Could not save theme preference. Reverted to your previous setting.";

type EffectiveTheme = "light" | "dark";

interface ThemeContextValue {
  preference: ThemePreference;
  effective: EffectiveTheme;
  setPreference: (next: ThemePreference) => Promise<void>;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

const BROADCAST_CHANNEL = "reverie-theme";

interface InitialState {
  preference: ThemePreference;
  effective: EffectiveTheme;
}

/** Synchronous initial-state derivation. preference comes from the cookie
 *  (so we keep "system" intent); effective comes from <html data-theme>
 *  (which the FOUC script already resolved), with a matchMedia fallback
 *  when dataset.theme is missing. */
function deriveInitialState(): InitialState {
  const stored = readThemeCookie();
  const preference: ThemePreference = stored ?? "system";

  const painted = document.documentElement.dataset.theme;
  let effective: EffectiveTheme;
  if (painted === "dark" || painted === "light") {
    effective = painted;
  } else if (preference === "system") {
    effective = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  } else {
    effective = preference;
  }
  return { preference, effective };
}

function applyEffective(theme: EffectiveTheme): void {
  document.documentElement.dataset.theme = theme;
}

function resolveEffective(preference: ThemePreference, systemDark: boolean): EffectiveTheme {
  if (preference === "system") return systemDark ? "dark" : "light";
  return preference;
}

interface ThemeProviderProps {
  children: ReactNode;
}

export function ThemeProvider({ children }: ThemeProviderProps): ReactElement {
  const initial = useMemo(() => deriveInitialState(), []);
  // Raw `useState` setters intentionally suffixed with `State` because the
  // public `setPreference` (line below) is the async callback that wraps
  // the optimistic-update + PATCH + rollback flow. Renaming would collide
  // with that public API.
  /* eslint-disable @eslint-react/use-state -- naming collides with public setPreference callback */
  const [preference, setPreferenceState] = useState<ThemePreference>(initial.preference);
  const [effective, setEffectiveState] = useState<EffectiveTheme>(initial.effective);
  /* eslint-enable @eslint-react/use-state */
  const channelRef = useRef<BroadcastChannel | null>(null);
  // Captures preference at mount so the reconcile effect can compare
  // server vs. mount-time without re-firing on every preference change.
  const mountPreferenceRef = useRef<ThemePreference>(initial.preference);

  // System-preference media query: keep `effective` in sync when the user
  // chose `system` and the OS toggles light/dark mid-session.
  useEffect(() => {
    const mql = matchMedia("(prefers-color-scheme: dark)");
    const onChange = (): void => {
      setPreferenceState((prefRef) => {
        if (prefRef === "system") {
          const next = resolveEffective("system", mql.matches);
          setEffectiveState(next);
          applyEffective(next);
        }
        return prefRef;
      });
    };
    mql.addEventListener("change", onChange);
    return () => {
      mql.removeEventListener("change", onChange);
    };
  }, []);

  // Reconcile with the server on mount. Logged-out visitors (401) stay on
  // the cookie-derived preference; transient errors are ignored (the
  // existing cookie is the authoritative fallback). Effect runs once
  // (empty deps); the comparison uses mountPreferenceRef so later
  // preference changes — which already flow through setPreference and
  // BroadcastChannel — don't re-fire the reconcile.
  useEffect(() => {
    const mountPreference = mountPreferenceRef.current;
    const controller = new AbortController();
    const reconcile = async (): Promise<void> => {
      try {
        const result = await fetchMe(controller.signal);
        if (controller.signal.aborted) return;
        if (result.kind !== "ok") return;
        if (result.theme_preference === mountPreference) return;
        const serverPref = result.theme_preference;
        const systemDark = matchMedia("(prefers-color-scheme: dark)").matches;
        const nextEffective = resolveEffective(serverPref, systemDark);
        writeThemeCookie(serverPref);
        applyEffective(nextEffective);
        setPreferenceState(serverPref);
        setEffectiveState(nextEffective);
      } catch (error) {
        if (error instanceof DOMException && error.name === "AbortError") {
          return;
        }
        // Reconciliation is best-effort; the cookie is the source of
        // truth for the FOUC and survives this failure.
        console.warn("theme reconciliation failed", error);
      }
    };
    void reconcile();
    return () => {
      controller.abort();
    };
  }, []);

  // Cross-tab sync: receive remote changes without re-PATCHing.
  useEffect(() => {
    const channel = new BroadcastChannel(BROADCAST_CHANNEL);
    channelRef.current = channel;
    // BroadcastChannel.close() in the cleanup releases the listener;
    // explicit removeEventListener + named handler would be redundant.
    // eslint-disable-next-line @eslint-react/web-api-no-leaked-event-listener -- channel.close() in cleanup releases the listener
    channel.addEventListener("message", (event) => {
      const msg = event.data as { preference?: unknown };
      const candidate = msg.preference;
      if (candidate !== "system" && candidate !== "light" && candidate !== "dark") {
        return;
      }
      const systemDark = matchMedia("(prefers-color-scheme: dark)").matches;
      const nextEffective = resolveEffective(candidate, systemDark);
      writeThemeCookie(candidate);
      applyEffective(nextEffective);
      setPreferenceState(candidate);
      setEffectiveState(nextEffective);
    });
    return () => {
      channel.close();
      channelRef.current = null;
    };
  }, []);

  const setPreference = useCallback(
    async (next: ThemePreference): Promise<void> => {
      const prevPreference = preference;
      const prevEffective = effective;
      const systemDark = matchMedia("(prefers-color-scheme: dark)").matches;
      const nextEffective = resolveEffective(next, systemDark);

      // Optimistic update: cookie + DOM + state immediately.
      writeThemeCookie(next);
      applyEffective(nextEffective);
      setPreferenceState(next);
      setEffectiveState(nextEffective);

      try {
        const result = await patchTheme(next);
        if (result.ok) {
          channelRef.current?.postMessage({ preference: next });
          return;
        }
        // 401 (anonymous) and 5xx (backend unavailable) are not real
        // failures — the cookie is the documented source of truth for
        // the theme preference and survives logout / backend outages by
        // design (see docs/.../design/visual-identity.md, § Theme
        // cookie lifecycle). Keep the optimistic update; emit a quiet
        // console warning so the situation is visible in devtools but
        // never surface a toast that contradicts the cookie's
        // authority. Validation rejections (4xx other than 401) still
        // roll back + toast — those represent a real disagreement
        // between client and server.
        if (result.status === 401 || result.status >= 500) {
          console.warn(`theme PATCH returned ${String(result.status)}; cookie persists`);
          // Cookie wins → broadcast so other tabs converge without a
          // reload. Same contract as the 2xx success path.
          channelRef.current?.postMessage({ preference: next });
          return;
        }
        writeThemeCookie(prevPreference);
        applyEffective(prevEffective);
        setPreferenceState(prevPreference);
        setEffectiveState(prevEffective);
        toast.error(PATCH_FAILURE_MESSAGE);
      } catch (error) {
        // Network-level failure (DNS, offline, fetch reject). Cookie
        // already holds the optimistic value; treat the same as a 5xx —
        // device-state semantics, not session-state. Broadcast so other
        // tabs converge without a reload.
        console.warn("theme PATCH failed; cookie persists", error);
        channelRef.current?.postMessage({ preference: next });
      }
    },
    [preference, effective],
  );

  const value = useMemo<ThemeContextValue>(
    () => ({ preference, effective, setPreference }),
    [preference, effective, setPreference],
  );

  return <ThemeContext value={value}>{children}</ThemeContext>;
}

// eslint-disable-next-line react-refresh/only-export-components -- co-locating the hook with the provider keeps the public API discoverable.
export function useTheme(): ThemeContextValue {
  const ctx = use(ThemeContext);
  if (!ctx) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return ctx;
}
