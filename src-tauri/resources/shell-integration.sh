# Terminal Grid shell integration for bash and zsh. Sourced at startup.
__tg_osc() { printf '\033]%s\007' "$1"; }
__tg_precmd() {
  local exit=$?
  local number last
  if [ -n "$ZSH_VERSION" ]; then
    number=$HISTCMD
  else
    # $HISTCMD reads 1 inside PROMPT_COMMAND on bash 3.2, so take the number `history 1` prints.
    read -r number last <<< "$(HISTTIMEFORMAT= history 1)"
  fi
  # The first prompt only records the number: the history file was loaded, not run.
  if [ -z "$__tg_seen" ]; then
    __tg_seen=1
    __tg_last_number=$number
  elif [ "$number" != "$__tg_last_number" ]; then
    __tg_last_number=$number
    if [ -n "$ZSH_VERSION" ]; then last=$(fc -ln -1 2>/dev/null); fi
    __tg_osc "7777;cmd;$(printf '%s' "$last" | base64 | tr -d '\n')"
    __tg_osc "133;D;$exit"
  fi
  __tg_osc "7;file://localhost$PWD"
  __tg_osc "133;A"
}
if [ -n "$ZSH_VERSION" ]; then
  precmd_functions+=(__tg_precmd)
else
  PROMPT_COMMAND="__tg_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
