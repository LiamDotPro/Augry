# Terminal Grid ZDOTDIR wrapper, part 2 of 4. See src/pty/shell.rs.
# Runs the user's own .zprofile with ZDOTDIR pointing at their folder, then puts ours back.
if [[ -n "$TG_USER_ZDOTDIR" && -r "$TG_USER_ZDOTDIR/.zprofile" ]]; then
  TG_WRAPPER_ZDOTDIR=$ZDOTDIR
  ZDOTDIR=$TG_USER_ZDOTDIR
  . "$TG_USER_ZDOTDIR/.zprofile"
  # The user's file may have moved ZDOTDIR; that is where their other files are.
  TG_USER_ZDOTDIR=$ZDOTDIR
  ZDOTDIR=$TG_WRAPPER_ZDOTDIR
  unset TG_WRAPPER_ZDOTDIR
fi
