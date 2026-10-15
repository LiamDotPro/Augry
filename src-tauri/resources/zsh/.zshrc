# Terminal Grid ZDOTDIR wrapper, part 3 of 4. See src/pty/shell.rs.
# Runs the user's own .zshrc, then the shell integration.
TG_WRAPPER_ZDOTDIR=$ZDOTDIR
# /etc/zshrc on macOS puts the history file under ZDOTDIR, which is ours at that point.
if [[ "$HISTFILE" == "$TG_WRAPPER_ZDOTDIR/.zsh_history" ]]; then
  HISTFILE="$TG_USER_ZDOTDIR/.zsh_history"
fi
if [[ -n "$TG_USER_ZDOTDIR" && -r "$TG_USER_ZDOTDIR/.zshrc" ]]; then
  ZDOTDIR=$TG_USER_ZDOTDIR
  . "$TG_USER_ZDOTDIR/.zshrc"
  TG_USER_ZDOTDIR=$ZDOTDIR
  ZDOTDIR=$TG_WRAPPER_ZDOTDIR
fi
[[ -r "$TG_SHELL_INTEGRATION" ]] && . "$TG_SHELL_INTEGRATION"
# A login shell still reads our .zlogin next, which restores ZDOTDIR for good.
if [[ ! -o login ]]; then
  ZDOTDIR=$TG_USER_ZDOTDIR
  unset TG_USER_ZDOTDIR TG_WRAPPER_ZDOTDIR TG_SHELL_INTEGRATION
fi
