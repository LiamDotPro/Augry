/**
 * Small pieces shared by the account and team screens (App Shell boards 06,
 * 07 and 08): the GitHub mark, stroke icons, the avatar and the team tile.
 * Sign in itself is GitHub's device flow (src/cloud/useDeviceSignIn.ts).
 */

/** GitHub's mark from Simple Icons (CC0), 24-unit grid, drawn in the text colour. */
const GITHUB_MARK =
  "M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12";

/** Stroke icons from the design's icon set (design/icons/paths.js). */
const STROKE = {
  mail: "M3.5 5.5h17v13h-17zM3.5 6.5l8.5 6.5 8.5-6.5",
  lock: "M6.5 10.5h11v9.5h-11zM8.5 10.5V8a3.5 3.5 0 0 1 7 0v2.5",
  folder: "M3.5 7a1.5 1.5 0 0 1 1.5-1.5h4.2l2 2.2H19a1.5 1.5 0 0 1 1.5 1.5v8.8a1.5 1.5 0 0 1-1.5 1.5H5a1.5 1.5 0 0 1-1.5-1.5z",
  plug: "M9 3v5M15 3v5M6.5 8h11v3a5.5 5.5 0 0 1-11 0zM12 16.5V21",
  monitor: "M4 5.5h16v10H4zM9 19.5h6M12 15.5v4",
  clock: "M12 7v5.5l3 2M12 3.5a8.5 8.5 0 1 0 0 17 8.5 8.5 0 0 0 0-17z",
  check: "M5 12.5l4.5 4.5L19 7.5",
} as const;

export type StrokeIconName = keyof typeof STROKE;

export function GitHubMark({ size = 17 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" focusable="false" className="account-icon">
      <path d={GITHUB_MARK} fill="currentColor" />
    </svg>
  );
}

export function StrokeIcon({ name, size = 16, weight = 1.7 }: { name: StrokeIconName; size?: number; weight?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" focusable="false" className="account-icon">
      <path
        d={STROKE[name]}
        fill="none"
        stroke="currentColor"
        strokeWidth={weight}
        strokeLinecap="round"
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

/** The first letter of a name, for avatars and team tiles. */
export function initialOf(name: string): string {
  return (name.trim().charAt(0) || "?").toUpperCase();
}

/** A person's initial on the avatar gradient. */
export function Avatar({ name, size = 20 }: { name: string; size?: number }) {
  return (
    <span className="cloud-avatar" style={{ ["--cloud-avatar-size" as string]: `${size}px` }} aria-hidden="true">
      {initialOf(name)}
    </span>
  );
}

/** A team's initial on the violet tile. */
export function TeamTile({ name, size = 18 }: { name: string; size?: number }) {
  return (
    <span className="cloud-team-tile" style={{ ["--cloud-tile-size" as string]: `${size}px` }} aria-hidden="true">
      {initialOf(name)}
    </span>
  );
}
