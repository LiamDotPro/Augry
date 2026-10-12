import { useEffect, useRef, useState, type CSSProperties } from "react";
import { useCloud } from "../../cloud/CloudProvider";
import { useDeviceSignIn } from "../../cloud/useDeviceSignIn";
import { cx } from "../../lib/cx";
import { GitHubMark, StrokeIcon } from "../Account";
import { codeMinutes } from "./accountText";

const WIDTH = { "--cloud-dialog-width": "600px" } as CSSProperties;

/**
 * Sign in with GitHub (App Shell board 07): sign-in explains what an account
 * adds, sign-in-code shows GitHub's device code while we wait for approval,
 * sign-in-expired offers a new code. A pending invite (`joinCode`) is
 * accepted by CloudProvider.completeSignIn once this succeeds.
 */
export function SignInDialog({ joinCode: _joinCode }: { joinCode?: string }) {
  const cloud = useCloud();
  const { state, start, cancel, reopen } = useDeviceSignIn();
  const primary = useRef<HTMLButtonElement>(null);
  const [copied, setCopied] = useState(false);
  // The expired code stays on screen while a new one is fetched.
  const [expiredCode, setExpiredCode] = useState<string | null>(null);
  // The code as last shown, so the dialog doesn't jump back while it closes.
  const [lastCode, setLastCode] = useState<{ userCode: string; minutes: number } | null>(null);

  useEffect(() => {
    if (state.step === "expired") setExpiredCode(state.userCode);
    if (state.step === "waiting" || state.step === "error") setExpiredCode(null);
    if (state.step === "waiting") {
      setLastCode({ userCode: state.userCode, minutes: codeMinutes(state.expiresAt, Date.now()) });
    }
  }, [state]);

  const view =
    state.step === "waiting" || (state.step === "done" && lastCode)
      ? "code"
      : state.step === "expired" || (state.step === "starting" && expiredCode)
        ? "expired"
        : "intro";
  const busy = state.step === "starting";

  const close = () => {
    cancel();
    cloud.closeDialog();
  };

  const onPrimary = () => {
    if (view === "code" || busy) return;
    void start();
  };

  // Escape closes, Enter takes the primary action; neither reaches the app below.
  const keys = useRef({ close, onPrimary });
  keys.current = { close, onPrimary };
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        keys.current.close();
      } else if (event.key === "Enter" && !(event.target as HTMLElement | null)?.closest("button")) {
        event.preventDefault();
        event.stopPropagation();
        keys.current.onPrimary();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);

  useEffect(() => {
    primary.current?.focus();
  }, [view]);

  const copy = (code: string) => {
    void navigator.clipboard.writeText(code).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    });
  };

  const title = view === "code" ? "Enter this code on GitHub" : view === "expired" ? "That code expired" : "Sign in with GitHub";
  const code = state.step === "waiting" ? state.userCode : (lastCode?.userCode ?? "");
  const minutes = lastCode?.minutes ?? 15;

  return (
    <div className="cloud-scrim" onMouseDown={close}>
      <div
        className="cloud-dialog"
        style={WIDTH}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="cloud-dialog__head">
          {view === "expired" ? (
            <span className="cloud-dialog__tile sign-in-dialog__tile--expired">
              <StrokeIcon name="clock" size={17} />
            </span>
          ) : (
            <span className="cloud-dialog__tile">
              <GitHubMark />
            </span>
          )}
          <h2 className="cloud-dialog__title">{title}</h2>
          <button type="button" className="cloud-dialog__close" aria-label="Close" onClick={close}>
            ×
          </button>
        </div>

        {view === "intro" && (
          <div className="cloud-dialog__body">
            <p className="cloud-dialog__text">
              Sign in to create or join a team and share processes and templates with it. Nothing else changes, and
              nothing you have now is uploaded.
            </p>
            <div className="sign-in-dialog__compare">
              <div className="sign-in-dialog__column sign-in-dialog__column--local">
                <div className="sign-in-dialog__column-title">Stays on this computer</div>
                <Perk ok>Terminals</Perk>
                <Perk ok>Your processes and runs</Perk>
                <Perk ok>Your connections and their keys</Perk>
              </div>
              <div className="sign-in-dialog__column">
                <div className="sign-in-dialog__column-title">Shared with a team</div>
                <Perk>Team processes and versions</Perk>
                <Perk>Templates added to the team</Perk>
                <Perk>Team connections, one set of keys</Perk>
                <Perk>Notes, synced to you or the team</Perk>
              </div>
            </div>
            <p className="cloud-dialog__small">
              Augry gets your GitHub name, login and email. It never sees your password.
            </p>
            {state.step === "error" && (
              <p className="cloud-field__error" role="alert">
                {state.message}
              </p>
            )}
          </div>
        )}

        {view === "code" && (
          <div className="cloud-dialog__body">
            <p className="cloud-dialog__text">
              We opened <strong>github.com/login/device</strong> in your browser. Enter the code there and approve
              Augry.
            </p>
            <div className="sign-in-dialog__code-card">
              <div className="sign-in-dialog__code-line">
                <span className="sign-in-code">{code}</span>
                <button type="button" className="sign-in-dialog__copy" onClick={() => copy(code)}>
                  {copied ? "Copied" : "Copy"}
                </button>
              </div>
              <div className="sign-in-status" role="status">
                <span className="sign-in-status__dot" aria-hidden="true" />
                Waiting for you to approve on GitHub
              </div>
            </div>
            <p className="cloud-dialog__small">
              The code works for {minutes} minutes.
              {cloud.deviceName && ` This device will show as ${cloud.deviceName} in your signed-in devices.`}
            </p>
          </div>
        )}

        {view === "expired" && (
          <div className="cloud-dialog__body">
            <p className="cloud-dialog__text">Codes work for 15 minutes. Get a new one and enter it on GitHub.</p>
            <div className="sign-in-dialog__code-card sign-in-dialog__code-card--expired">
              <span className="sign-in-code sign-in-code--expired">{expiredCode}</span>
            </div>
          </div>
        )}

        <div className="cloud-dialog__foot">
          <button type="button" className="cloud-btn cloud-btn--quiet" onClick={close}>
            Cancel
          </button>
          <span className="cloud-dialog__spacer" />
          {view === "code" ? (
            <button ref={primary} type="button" className="cloud-btn sign-in-dialog__again" onClick={reopen}>
              Open GitHub again
            </button>
          ) : (
            <button
              ref={primary}
              type="button"
              className={cx("cloud-btn", "cloud-btn--primary", busy && "sign-in-dialog__busy")}
              aria-busy={busy || undefined}
              onClick={onPrimary}
            >
              {view === "expired" ? (
                "Get a new code"
              ) : (
                <>
                  <GitHubMark size={15} />
                  {state.step === "error" ? "Try again" : "Continue with GitHub"}
                </>
              )}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function Perk({ ok = false, children }: { ok?: boolean; children: string }) {
  return (
    <div className="sign-in-dialog__perk">
      <span className={cx("sign-in-dialog__mark", ok && "sign-in-dialog__mark--ok")} aria-hidden="true">
        {ok ? "✓" : "+"}
      </span>
      {children}
    </div>
  );
}
