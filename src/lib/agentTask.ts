/**
 * The text Augry hands an agent so it reports its current task
 * (design section 5.5). Every shell gets TERMINAL_GRID_TASK_FILE; the agent
 * writes one line there and the pane's task overlay shows it.
 */

/**
 * Typed into the pane by "Ask agent". One line on purpose: a newline would
 * submit the prompt half way through.
 */
export const TASK_REPORT_PROMPT =
  "Augry: write one short line saying what you are working on right now to the file " +
  "named by the TERMINAL_GRID_TASK_FILE environment variable " +
  '(for example: echo "Fixing the login test" > "$TERMINAL_GRID_TASK_FILE"), ' +
  "and rewrite it whenever your task changes.";

/** For CLAUDE.md or AGENTS.md, so agents report without being asked. */
export const TASK_REPORT_INSTRUCTIONS = `## Augry

When the \`TERMINAL_GRID_TASK_FILE\` environment variable is set, keep a one line
summary of your current task in that file, and rewrite it whenever the task
changes:

    echo "Adding retries to the upload client" > "$TERMINAL_GRID_TASK_FILE"
`;
