/** Command lines Augry types into a shell on the user's behalf. */
import type { ShellKind } from "../ipc/types";

/**
 * The key that empties a half typed prompt line. Send it on its own: an
 * Escape followed straight away by text reads as Alt+key.
 */
export function clearLineKey(shell: ShellKind): string | null {
  switch (shell) {
    case "pwsh":
    case "windowsPowerShell":
    case "cmd":
      return "\x1b";
    case "bash":
    case "zsh":
      return "\x15";
    default:
      return null;
  }
}

/**
 * A directory change in the shell's own syntax, quoted so paths with spaces
 * or quotes survive. The caller sends the Enter.
 */
export function cdCommand(shell: ShellKind, path: string): string {
  switch (shell) {
    case "pwsh":
    case "windowsPowerShell":
      return `Set-Location -LiteralPath '${path.replace(/'/g, "''")}'`;
    case "cmd":
      // cmd wants backslashes, and /d to switch drives.
      return `cd /d "${path.replace(/\//g, "\\")}"`;
    case "bash":
    case "zsh":
      // Close the quote, escape one, reopen.
      return `cd '${path.replace(/'/g, "'\\''")}'`;
    default:
      return `cd "${path}"`;
  }
}
