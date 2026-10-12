import { useCallback, useEffect, useState, type ReactNode } from "react";
import type { HotkeyModifier } from "../ipc/types";
import { cx } from "../lib/cx";
import { IS_MAC, modifierLabel } from "../lib/hotkeys";
import { useAppActions, useAppState } from "../state/AppProvider";
import { useCloud } from "../cloud/CloudProvider";
import { GitHubMark, StrokeIcon } from "./Account";
import { AgentIcon } from "./AgentIcon";
import { FONTS, SHELLS } from "./SettingsDialog";

const STEPS = [
  { title: "Tools" },
  { title: "Preferences" },
  { title: "Account", optional: true },
] as const;

/** The sizes setup offers; Settings takes any size. */
const FONT_SIZES = [13, 14, 15, 16];

/** Setup offers the first three fonts; the rest are in Settings. */
const SETUP_FONTS = FONTS.slice(0, 3);

/**
 * First-time setup (App Shell.dc.html board 06). Runs once, after the startup
 * animation: the shell and agents on this computer, where notes go and the
 * terminal font, then an optional account. Defaults are already picked, every
 * choice can be changed later in Settings, and it ends on the Terminals tab.
 */
export function FirstRunSetup() {
  const state = useAppState();
  const actions = useAppActions();
  const [step, setStep] = useState(0);
  const [shell, setShell] = useState(state.config.shell ?? "");
  const [modifier, setModifier] = useState<HotkeyModifier>(state.config.hotkeyModifier);
  const [patterns, setPatterns] = useState(state.config.agentPatterns);
  const [adding, setAdding] = useState<string | null>(null);
  const [notesRoot, setNotesRoot] = useState(state.config.notesRoot ?? state.notes.root);
  const [fontFamily, setFontFamily] = useState(state.config.fontFamily ?? FONTS[0]);
  const [fontSize, setFontSize] = useState(state.config.fontSize);
  const cloud = useCloud();

  const finish = useCallback(async () => {
    await actions.saveConfig({ shell: shell || null, hotkeyModifier: modifier, agentPatterns: patterns, fontFamily, fontSize });
    if (notesRoot && notesRoot !== (state.config.notesRoot ?? state.notes.root)) await actions.setNotesRoot(notesRoot);
    actions.setTab("terminals");
    actions.finishSetup();
  }, [actions, shell, modifier, patterns, fontFamily, fontSize, notesRoot, state.config.notesRoot, state.notes.root]);

  // TODO(account): setup-account and setup-browser (show the code here with useDeviceSignIn).
  const browserPending = false;

  // Enter continues, the way the button says.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // A focused control keeps its own Enter: Back goes back, Change opens the picker.
      const target = event.target as HTMLElement | null;
      if (event.key !== "Enter" || step >= 2 || target?.closest("input, button, select, textarea, a")) return;
      event.preventDefault();
      setStep((current) => current + 1);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [step]);

  const agents = state.installedAgents;
  const mod = modifierLabel(IS_MAC ? "mac" : modifier);

  return (
    <div className="setup" role="dialog" aria-modal="true" aria-label="Set up Augry">
      <div className="setup__card">
        <div className="setup__main">
          <ol className="setup__steps">
            {STEPS.map(({ title, ...rest }, index) => (
              <li
                key={title}
                className={cx(
                  "setup__step",
                  index <= step && "setup__step--reached",
                  index === step && "setup__step--current",
                )}
                aria-current={index === step ? "step" : undefined}
              >
                <span className="setup__step-bar" />
                <span className="setup__step-label">
                  <span className="setup__step-n">{index + 1}</span>
                  {title}
                  {"optional" in rest && <span className="setup__step-optional">optional</span>}
                </span>
              </li>
            ))}
          </ol>

          {step === 0 && (
            <>
              <Heading title="Welcome to Augry">
                We looked for coding agents on this computer and picked sensible defaults. Change anything later in
                Settings.
              </Heading>
              <div className="setup__columns">
                <section className="setup__group" aria-label="Shell">
                  <GroupTitle title="Shell" />
                  <div role="radiogroup" aria-label="Shell" className="setup__options">
                    {SHELLS.map((option, index) => (
                      <button
                        key={option.value}
                        type="button"
                        role="radio"
                        aria-checked={shell === option.value}
                        className={cx("setup-option", shell === option.value && "setup-option--selected")}
                        onClick={() => setShell(option.value)}
                      >
                        <span className="setup-option__radio" aria-hidden="true" />
                        <span className="setup-option__text">
                          <span className="setup-option__name">{option.value === "" ? "Auto" : option.label}</span>
                          <span className="setup-option__detail">
                            {option.value === "" ? option.label.replace(/^Auto \((.*)\)$/, "$1") : option.value}
                          </span>
                        </span>
                        {index === 0 && <span className="setup-option__tag">Recommended</span>}
                      </button>
                    ))}
                  </div>
                </section>

                <section className="setup__group" aria-label="Coding agents">
                  <GroupTitle title="Coding agents" count={`${agents.length} found`} />
                  {agents.map((agent) => (
                    <div key={agent} className="setup-option setup-option--static">
                      <span className="setup-option__glyph setup-option__glyph--agent">
                        <AgentIcon agent={agent} size={17} />
                      </span>
                      <span className="setup-option__text">
                        <span className="setup-option__name">{agentName(agent)}</span>
                        <span className="setup-option__detail">{agent} · on PATH</span>
                      </span>
                      <span className="setup-option__tag setup-option__tag--ok">✓ Found</span>
                    </div>
                  ))}
                  {patterns
                    .filter((pattern) => !state.config.agentPatterns.includes(pattern))
                    .map((pattern) => (
                      <div key={pattern} className="setup-option setup-option--static">
                        <span className="setup-option__glyph">
                          <AgentIcon agent={pattern} size={17} />
                        </span>
                        <span className="setup-option__text">
                          <span className="setup-option__name">{pattern}</span>
                          <span className="setup-option__detail">Added · marks panes running it as an agent</span>
                        </span>
                      </div>
                    ))}
                  {adding === null ? (
                    <button type="button" className="setup-option setup-option--add" onClick={() => setAdding("")}>
                      <span className="setup-option__glyph">
                        <StrokeIcon name="plug" />
                      </span>
                      <span className="setup-option__text">
                        <span className="setup-option__name setup-option__name--plain">Add another agent</span>
                        <span className="setup-option__detail">Any command, like aider</span>
                      </span>
                    </button>
                  ) : (
                    <div className="setup-option setup-option--add">
                      <span className="setup-option__glyph">
                        <StrokeIcon name="plug" />
                      </span>
                      <input
                        autoFocus
                        className="setup-option__input"
                        placeholder="Command name, like aider"
                        aria-label="Agent command"
                        value={adding}
                        onChange={(event) => setAdding(event.target.value)}
                        onKeyDown={(event) => {
                          if (event.key === "Escape") setAdding(null);
                          if (event.key !== "Enter") return;
                          const value = adding.trim();
                          if (value && !patterns.includes(value)) setPatterns([...patterns, value]);
                          setAdding(null);
                        }}
                        onBlur={() => setAdding(null)}
                      />
                    </div>
                  )}
                  <div className="setup__note">Panes running these get the finished glow and can receive review comments.</div>
                </section>
              </div>

              {!IS_MAC && (
                <div className="setup__inline">
                  <GroupTitle title="Hotkeys" />
                  <div className="segmented" role="radiogroup" aria-label="Hotkey modifier">
                    {(["ctrl+alt", "ctrl+shift"] as HotkeyModifier[]).map((value) => (
                      <button
                        key={value}
                        type="button"
                        role="radio"
                        aria-checked={modifier === value}
                        className={cx("segmented__item", modifier === value && "segmented__item--active")}
                        onClick={() => setModifier(value)}
                      >
                        {modifierLabel(value)}
                      </button>
                    ))}
                  </div>
                  <span className="setup__note">
                    {mod}+Arrow moves panes · {mod}+Enter opens review
                  </span>
                </div>
              )}
            </>
          )}

          {step === 1 && (
            <>
              <Heading title="Notes and terminal">
                Where saved notes go and how terminals look. After setup you open your first terminal in any folder from
                the Terminals tab.
              </Heading>
              <div className="setup__columns">
                <section className="setup__group" aria-label="Notes folder">
                  <GroupTitle title="Notes folder" />
                  <div className="setup-option setup-option--static">
                    <span className="setup-option__glyph setup-option__glyph--folder">
                      <StrokeIcon name="folder" />
                    </span>
                    <span className="setup-option__text">
                      <span className="setup-option__name" title={notesRoot ?? ""}>
                        {notesRoot ?? "Not set"}
                      </span>
                      <span className="setup-option__detail">Markdown files</span>
                    </span>
                    <button
                      type="button"
                      className="btn btn--outline"
                      onClick={() => {
                        void actions.chooseNotesRoot().then((root) => {
                          if (root) setNotesRoot(root);
                        });
                      }}
                    >
                      Change
                    </button>
                  </div>
                  <div className="setup__note">Text you save from a pane goes to notes/&lt;repo&gt;/.</div>
                </section>

                <section className="setup__group" aria-label="Terminal font">
                  <GroupTitle title="Terminal font" />
                  <div className="segmented" role="radiogroup" aria-label="Font">
                    {SETUP_FONTS.map((font) => (
                      <button
                        key={font}
                        type="button"
                        role="radio"
                        aria-checked={fontFamily === font}
                        className={cx("segmented__item", fontFamily === font && "segmented__item--active")}
                        onClick={() => setFontFamily(font)}
                      >
                        {font}
                      </button>
                    ))}
                  </div>
                  <div className="setup__inline">
                    <div className="segmented" role="radiogroup" aria-label="Font size">
                      {FONT_SIZES.map((size) => (
                        <button
                          key={size}
                          type="button"
                          role="radio"
                          aria-checked={fontSize === size}
                          className={cx("segmented__item", fontSize === size && "segmented__item--active")}
                          onClick={() => setFontSize(size)}
                        >
                          {size}
                        </button>
                      ))}
                    </div>
                    <span className="setup__note">size in px</span>
                  </div>
                </section>
              </div>
            </>
          )}

          {step === 2 && (
            <>
              <Heading title="Sign in to share with a team">
                An account is optional. Everything works without one and stays on this computer. Sign in only if you
                want to create or join a team. You can do it later from Settings.
              </Heading>
              <div className="setup__columns">
                <div className="setup__compare">
                  <GroupTitle title="Works without an account, offline" />
                  <Perk ok>Terminals, the grid and every agent</Perk>
                  <Perk ok>Reviewing changes and requesting fixes</Perk>
                  <Perk ok>Notes, and saving agent output to them</Perk>
                  <Perk ok>Your own processes, runs and connections</Perk>
                  <div className="setup__rule" />
                  <GroupTitle title="With an account and a team" />
                  <Perk>Share processes with your team</Perk>
                  <Perk>Version history the whole team can see</Perk>
                  <Perk>Templates added to the team</Perk>
                </div>
                <div className="setup__group">
                  <button
                    type="button"
                    className="sign-in__button sign-in__button--primary"
                    onClick={() => cloud.openDialog({ kind: "sign-in" })}
                  >
                    <GitHubMark />
                    <span className="sign-in__label">Continue with GitHub</span>
                  </button>
                  <div className="setup__note">
                    GitHub opens in your browser and shows a short code to enter. Augry never sees your password.
                  </div>
                </div>
              </div>
            </>
          )}
        </div>

        <div className="setup__foot">
          {step > 0 && (
            <button
              type="button"
              className="btn btn--quiet setup__back"
              onClick={() => setStep(step - 1)}
            >
              Back
            </button>
          )}
          <span className="setup__spacer" />
          {step < 2 ? (
            <button type="button" className="setup__continue" onClick={() => setStep(step + 1)}>
              Continue
              <span className="setup__key">Enter</span>
            </button>
          ) : (
            <>
              {!browserPending && <span className="setup__note">Then opens the Terminals tab</span>}
              <button type="button" className="btn btn--outline setup__skip" onClick={() => void finish()}>
                Skip for now
              </button>
            </>
          )}
        </div>
      </div>

    </div>
  );
}

function Heading({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="setup__heading">
      <h2 className="setup__title">{title}</h2>
      <p className="setup__lede">{children}</p>
    </div>
  );
}

function GroupTitle({ title, count }: { title: string; count?: string }) {
  return (
    <div className="setup__group-title">
      {title}
      {count && <span className="setup__group-count">{count}</span>}
    </div>
  );
}

function Perk({ ok = false, children }: { ok?: boolean; children: ReactNode }) {
  return (
    <div className="setup__perk">
      <span className={cx("setup__perk-mark", ok && "setup__perk-mark--ok")} aria-hidden="true">
        {ok ? "✓" : "+"}
      </span>
      {children}
    </div>
  );
}

/** "claude" reads as "Claude Code", the product name; others are capitalised. */
function agentName(command: string): string {
  if (command === "claude") return "Claude Code";
  return command.charAt(0).toUpperCase() + command.slice(1);
}
