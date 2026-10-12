# Terminal Grid ZDOTDIR wrapper, part 4 of 4. See src/pty/shell.rs.
# Runs the user's own .zlogin, then hands ZDOTDIR back to the user for good.
if [[ -n "$TG_USER_ZDOTDIR" && -r "$TG_USER_ZDOTDIR/.zlogin" ]]; then
  ZDOTDIR=$TG_USER_ZDOTDIR
  . "$TG_USER_ZDOTDIR/.zlogin"
  TG_USER_ZDOTDIR=$ZDOTDIR
fi
ZDOTDIR=$TG_USER_ZDOTDIR
unset TG_USER_ZDOTDIR TG_WRAPPER_ZDOTDIR TG_SHELL_INTEGRATION
