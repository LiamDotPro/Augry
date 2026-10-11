import { useCallback, useState } from "react";
import type { Account } from "../ipc/types";
import { BrowserWait, EmailSignInDialog, SignInButtons, StrokeIcon } from "./Account";

/** The templates a signed-out user is shown, as names only. */
const TEMPLATE_NAMES = ["Feature delivery", "Bug fix", "Parallel attempts", "Dependency update", "Visual check", "Release"];

/**
 * The Process tab while signed out (App Shell.dc.html, process-signed-out):
 * how to sign in instead of the processes. Terminals and Notes never ask.
 */
export function ProcessSignedOut() {
  const [pending, setPending] = useState<Account["provider"] | null>(null);
  const reset = useCallback(() => setPending(null), []);

  return (
    <div className="signed-out">
      <div className="signed-out__card">
        <span className="signed-out__badge">
          <StrokeIcon name="lock" size={24} />
        </span>
        <div className="signed-out__intro">
          <h2 className="signed-out__title">
            {pending === "github" || pending === "google" ? "Finish signing in your browser" : "Processes need an account"}
          </h2>
          <p className="signed-out__text">
            Sign in to run processes and templates, and connect GitHub, Linear, Jira and databases. Terminals and Notes
            keep working without one.
          </p>
        </div>

        {pending === "github" || pending === "google" ? (
          <BrowserWait provider={pending} onSignedIn={reset} onCancel={reset} />
        ) : (
          <SignInButtons onPick={setPending} />
        )}

        <div className="signed-out__templates">
          <div className="signed-out__templates-label">Templates you can run once you sign in</div>
          <div className="signed-out__chips">
            {TEMPLATE_NAMES.map((name) => (
              <span key={name} className="signed-out__chip">
                {name}
              </span>
            ))}
          </div>
        </div>
      </div>

      {pending === "email" && <EmailSignInDialog onClose={reset} />}
    </div>
  );
}
