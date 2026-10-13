# Augry shell integration for bash and zsh. Sourced at startup.
__tg_osc() { printf '\033]%s\007' "$1"; }
__tg_precmd() {
  local exit=$?
  local last
  if [ -n "$ZSH_VERSION" ]; then last=$(fc -ln -1 2>/dev/null); else last=$(HISTTIMEFORMAT= history 1 | sed 's/^ *[0-9]* *//'); fi
  if [ -n "$last" ] && [ "$last" != "$__tg_last_cmd" ]; then
    __tg_last_cmd=$last
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
