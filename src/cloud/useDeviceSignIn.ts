import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "./api";
import { useCloud } from "./CloudProvider";

/**
 * GitHub's device flow, for the sign-in dialog and first-time setup
 * (App Shell boards 06 and 07): get a code, open github.com/login/device,
 * poll until the user approves, then hand the session to the CloudProvider.
 */
export type DeviceSignIn =
  | { step: "idle" }
  | { step: "starting" }
  | { step: "waiting"; userCode: string; verificationUri: string; expiresAt: number }
  /** sign-in-expired: the old code, struck through. */
  | { step: "expired"; userCode: string }
  | { step: "error"; message: string }
  | { step: "done" };

/** The design's copy for each way sign in can fail. */
export function signInErrorMessage(error: unknown): string {
  if (!api.isCloudError(error)) return "Something went wrong. Try again.";
  if (error.status === 403) return "GitHub said no. Try again if that was a mistake.";
  if (error.status === 422) return "Your GitHub account needs a verified primary email.";
  if (error.status === 503) return "Signing in isn't available right now. Try again later.";
  if (error.status === 429) return "Too many tries. Wait a minute and try again.";
  return "Can't reach GitHub right now. Try again in a moment.";
}

export function useDeviceSignIn() {
  const cloud = useCloud();
  const [state, setState] = useState<DeviceSignIn>({ step: "idle" });
  // Each start bumps the run, so an old poll loop stops by itself.
  const run = useRef(0);
  const complete = useRef(cloud.completeSignIn);
  complete.current = cloud.completeSignIn;

  const cancel = useCallback(() => {
    run.current += 1;
    setState({ step: "idle" });
  }, []);

  useEffect(
    () => () => {
      run.current += 1;
    },
    [],
  );

  const start = useCallback(async () => {
    const id = ++run.current;
    setState({ step: "starting" });
    let started: Awaited<ReturnType<typeof api.startDeviceFlow>>;
    try {
      started = await api.startDeviceFlow();
    } catch (error) {
      if (run.current === id) setState({ step: "error", message: signInErrorMessage(error) });
      return;
    }
    if (run.current !== id) return;
    const expiresAt = Date.now() + started.expiresIn * 1000;
    setState({ step: "waiting", userCode: started.userCode, verificationUri: started.verificationUri, expiresAt });

    let interval = started.interval;
    while (run.current === id) {
      await new Promise((resolve) => window.setTimeout(resolve, Math.max(1, interval) * 1000));
      if (run.current !== id) return;
      try {
        const poll = await api.pollDeviceFlow(started.handle);
        if (run.current !== id) return;
        if (poll.status === "pending") {
          // GitHub can ask us to slow down.
          interval = poll.interval;
          continue;
        }
        setState({ step: "done" });
        await complete.current(poll.session);
        return;
      } catch (error) {
        if (run.current !== id) return;
        if (api.isCloudError(error) && (error.status === 410 || error.status === 404)) {
          setState({ step: "expired", userCode: started.userCode });
          return;
        }
        // A blip while waiting: keep polling until the code runs out.
        if (api.isCloudError(error) && error.offline && Date.now() < expiresAt) continue;
        setState({ step: "error", message: signInErrorMessage(error) });
        return;
      }
    }
  }, []);

  /** Opens the code page again; the code stays the same. */
  const reopen = useCallback(() => {
    if (state.step === "waiting") void api.openUrl(state.verificationUri).catch(() => {});
  }, [state]);

  return { state, start, cancel, reopen };
}
