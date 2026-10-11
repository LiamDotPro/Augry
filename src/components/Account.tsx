import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { Account } from "../ipc/types";
import { cx } from "../lib/cx";
import { useAppActions } from "../state/AppProvider";

/**
 * Sign-in pieces shared by first-time setup, the signed-out Process tab and
 * Settings (App Shell.dc.html boards 06 and 07). There is no account service
 * yet: GitHub and Google stand in for the browser hand-off, email for the
 * one-time code, and all of them end in `signIn` with a mocked account.
 */

/** Brand marks from Simple Icons (CC0), 24-unit grid, drawn in the text colour. */
const BRAND: Record<"github" | "google", string> = {
  github: "M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12",
  google: "M12.48 10.92v3.28h7.84c-.24 1.84-.853 3.187-1.787 4.133-1.147 1.147-2.933 2.4-6.053 2.4-4.827 0-8.6-3.893-8.6-8.72s3.773-8.72 8.6-8.72c2.6 0 4.507 1.027 5.907 2.347l2.307-2.307C18.747 1.44 16.133 0 12.48 0 5.867 0 .307 5.387.307 12s5.56 12 12.173 12c3.573 0 6.267-1.173 8.373-3.36 2.16-2.16 2.84-5.213 2.84-7.667 0-.76-.053-1.467-.173-2.053H12.48z",
};

/** Stroke icons from the design's icon set (icons/paths.js). */
const STROKE = {
  mail: "M3.5 5.5h17v13h-17zM3.5 6.5l8.5 6.5 8.5-6.5",
  lock: "M6.5 10.5h11v9.5h-11zM8.5 10.5V8a3.5 3.5 0 0 1 7 0v2.5",
  folder: "M3.5 7a1.5 1.5 0 0 1 1.5-1.5h4.2l2 2.2H19a1.5 1.5 0 0 1 1.5 1.5v8.8a1.5 1.5 0 0 1-1.5 1.5H5a1.5 1.5 0 0 1-1.5-1.5z",
  plug: "M9 3v5M15 3v5M6.5 8h11v3a5.5 5.5 0 0 1-11 0zM12 16.5V21",
} as const;

export function ProviderIcon({ provider, size = 17 }: { provider: Account["provider"]; size?: number }) {
  if (provider === "email") return <StrokeIcon name="mail" size={size} />;
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" focusable="false" className="account-icon">
      <path d={BRAND[provider]} fill="currentColor" />
    </svg>
  );
}

export function StrokeIcon({ name, size = 16 }: { name: keyof typeof STROKE; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" focusable="false" className="account-icon">
      <path
        d={STROKE[name]}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

export const PROVIDER_LABELS: Record<Account["provider"], string> = {
  github: "GitHub",
  google: "Google",
  email: "email",
};

/** The three "Continue with …" buttons; GitHub is the primary one. */
export function SignInButtons({ onPick }: { onPick(provider: Account["provider"]): void }) {
  return (
    <div className="sign-in">
      {(["github", "google", "email"] as const).map((provider) => (
        <button
          key={provider}
          type="button"
          className={cx("sign-in__button", provider === "github" && "sign-in__button--primary")}
          onClick={() => onPick(provider)}
        >
          <ProviderIcon provider={provider} />
          <span className="sign-in__label">Continue with {PROVIDER_LABELS[provider]}</span>
          <span className="sign-in__chevron" aria-hidden="true">
            ›
          </span>
        </button>
      ))}
    </div>
  );
}

/** The round initial shown for a signed-in account. */
export function AccountAvatar({ account }: { account: Account }) {
  return (
    <span className="account-avatar" aria-hidden="true">
      {account.name.charAt(0).toUpperCase()}
    </span>
  );
}

const CODE_LENGTH = 6;

/**
 * Sign in with email: ask for the address, then the 6-digit code that was
 * mailed to it. Any six digits are accepted until the account service exists.
 */
export function EmailSignInDialog({ onClose, onSignedIn }: { onClose(): void; onSignedIn?(): void }) {
  const actions = useAppActions();
  const [email, setEmail] = useState("");
  const [sentTo, setSentTo] = useState<string | null>(null);
  const [code, setCode] = useState("");
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    input.current?.focus();
  }, [sentTo]);

  useEffect(() => {
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [onClose]);

  const validEmail = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim());
  const complete = code.length === CODE_LENGTH;

  const submit = () => {
    if (!sentTo) {
      if (validEmail) setSentTo(email.trim());
      return;
    }
    if (!complete) return;
    actions.signIn("email", sentTo);
    onSignedIn?.();
    onClose();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      submit();
    }
  };

  return (
    <div className="scrim scrim--dim" onMouseDown={onClose}>
      <div
        className="email-sign-in"
        role="dialog"
        aria-modal="true"
        aria-label="Sign in with email"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="email-sign-in__head">
          <span className="email-sign-in__icon">
            <StrokeIcon name="mail" />
          </span>
          <h2 className="email-sign-in__title">Sign in with email</h2>
          <button type="button" className="sheet__close" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>

        <div className="email-sign-in__body">
          {sentTo === null ? (
            <>
              <div className="email-sign-in__text">We’ll email you a 6-digit code. No password needed.</div>
              <input
                ref={input}
                className="email-sign-in__email"
                type="email"
                placeholder="you@example.com"
                aria-label="Email address"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                onKeyDown={onKeyDown}
              />
            </>
          ) : (
            <>
              <div className="email-sign-in__text">
                We sent a 6-digit code to <strong>{sentTo}</strong>. It works for 10 minutes.
              </div>
              {/* One real input under six drawn boxes, so paste and backspace just work. */}
              <label className="code-boxes">
                <input
                  ref={input}
                  className="code-boxes__input"
                  inputMode="numeric"
                  autoComplete="one-time-code"
                  aria-label="6-digit code"
                  maxLength={CODE_LENGTH}
                  value={code}
                  onChange={(event) => setCode(event.target.value.replace(/\D/g, "").slice(0, CODE_LENGTH))}
                  onKeyDown={onKeyDown}
                />
                {Array.from({ length: CODE_LENGTH }, (_, index) => (
                  <span
                    key={index}
                    className={cx("code-boxes__box", index === Math.min(code.length, CODE_LENGTH - 1) && "code-boxes__box--current")}
                    aria-hidden="true"
                  >
                    {code[index] ?? (index === code.length ? <span className="code-boxes__caret" /> : null)}
                  </span>
                ))}
              </label>
              <div className="email-sign-in__links">
                <button type="button" className="link-button" onClick={() => setCode("")}>
                  Resend code
                </button>
                <button
                  type="button"
                  className="link-button"
                  onClick={() => {
                    setSentTo(null);
                    setCode("");
                  }}
                >
                  Use a different email
                </button>
              </div>
            </>
          )}
        </div>

        <div className="email-sign-in__foot">
          <button type="button" className="btn btn--quiet" onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className="btn btn--primary"
            disabled={sentTo === null ? !validEmail : !complete}
            onClick={submit}
          >
            {sentTo === null ? "Send code" : "Sign in"}
          </button>
        </div>
      </div>
    </div>
  );
}

/** How long the mocked browser hand-off takes to "approve". */
const MOCK_BROWSER_MS = 2500;

/** A short code the browser page shows too, so the two can be matched. */
function handoffCode(): string {
  return `AUGR-${String(Math.floor(1000 + Math.random() * 9000))}`;
}

/**
 * "Finish signing in your browser": the code to match and a waiting line.
 * With no account service the browser never opens, so the wait resolves by
 * itself after a moment.
 */
export function BrowserWait({
  provider,
  onSignedIn,
  onCancel,
}: {
  provider: "github" | "google";
  onSignedIn(): void;
  onCancel(): void;
}) {
  const actions = useAppActions();
  const [code, setCode] = useState(handoffCode);
  const [attempt, setAttempt] = useState(0);
  // The parent re-renders every clock tick; a fresh callback must not restart the wait.
  const signedIn = useRef(onSignedIn);
  signedIn.current = onSignedIn;

  useEffect(() => {
    const timer = window.setTimeout(() => {
      actions.signIn(provider);
      signedIn.current();
    }, MOCK_BROWSER_MS);
    return () => window.clearTimeout(timer);
  }, [actions, provider, attempt]);

  return (
    <div className="browser-wait" role="status">
      <span className="browser-wait__mark">
        <ProviderIcon provider={provider} size={24} />
      </span>
      <div className="browser-wait__hint">Check your browser shows this code</div>
      <div className="browser-wait__code">{code}</div>
      <div className="browser-wait__status">
        <span className="browser-wait__dot" aria-hidden="true" />
        Waiting for {PROVIDER_LABELS[provider]}
      </div>
      <div className="browser-wait__actions">
        <button
          type="button"
          className="btn btn--outline"
          onClick={() => {
            setCode(handoffCode());
            setAttempt((n) => n + 1);
          }}
        >
          Open browser again
        </button>
        <button type="button" className="btn btn--quiet" onClick={onCancel}>
          Use another way
        </button>
      </div>
    </div>
  );
}
