#!/bin/zsh
# Full dictation test suite against the running app.
# Start the app first with:  UTTER_TEST_WAV=$PWD/scripts/e2e/fixtures/current.wav npm run tauri dev
# The terminal needs Accessibility + Input Monitoring. Pastes into a small test window (target.swift) and Chrome.
# Usage: UTTER_LOG=<path to the dev server log> scripts/e2e/suite.sh [case-id-filter]
# UTTER_LOG lets "nothing was pasted" checks read the app's own paste log. Without it they fall back to
# checking that the document stayed empty, which macOS Dictation can break if fn is set to start it.
setopt NULL_GLOB
cd "${0:A:h}"
FILTER="${1:-}"
DB="$HOME/Library/Application Support/dev.abdallah.utter/app.db"
FX=fixtures
K=./keys
mkdir -p $FX
[[ -x keys ]] || swiftc -O keys.swift -o keys
[[ -x UtterTestTarget ]] || swiftc -O target.swift -o UtterTestTarget
OUT=$(mktemp -t utter-target)

# --- fixtures (generated once, not committed) ---------------------------------
mk() { # mk <name> <voice> <text>
  [[ -f $FX/$1.wav ]] && return
  say -v "$2" -o $FX/$1.aiff "$3" && afconvert -f WAVE -d LEI16@16000 -c 1 $FX/$1.aiff $FX/$1.wav && rm $FX/$1.aiff
}
mk speech Samantha "Um, so the the meeting is at 5, actually 6 p m. Please send the notes to my email. Thanks a lot."
mk short Samantha "Yes."
mk german Anna "Grüße aus München. Das Treffen ist morgen um drei Uhr."
mk raw Samantha "Um, I think, uh, we should ship it."
mk correction Samantha "We use react query for data fetching."
mk long Samantha "$(for i in 1 2 3 4 5 6; do print -n "This is sentence number $i of a long dictation that checks how the app handles a full minute of speech without dropping words. "; done)"
[[ -f $FX/silence.wav ]] || python3 -c "
import wave; w=wave.open('$FX/silence.wav','wb'); w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000); w.writeframes(b'\x00\x00'*16000*2); w.close()"
[[ -f $FX/pixel.png ]] || python3 -c "
import base64; open('$FX/pixel.png','wb').write(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='))"

# --- helpers --------------------------------------------------------------------
use() { ln -sf "${PWD}/$FX/$1.wav" $FX/current.wav; }
q() { sqlite3 "$DB" "$1"; }
rows() { q "select count(*) from transcripts"; }
last() { q "select $1 from transcripts order by created_at desc, id desc limit 1"; }
setting() { q "update settings set $1 where id = 1"; }
reset_settings() { setting "cleanup_level='light', llm_provider='none', storage_mode='store', languages='[\"en\"]', auto_detect_language=0"; }
# Each case gets a fresh test window; it is killed afterwards, so nothing stays open or gets saved.
new_doc() { pkill -x UtterTestTarget 2>/dev/null; sleep 0.3; ./UtterTestTarget "$OUT" & sleep 1.5; }
close_docs() { pkill -x UtterTestTarget 2>/dev/null; rm -f "$OUT"; }
trap close_docs EXIT
doc() { cat "$OUT" 2>/dev/null; }
hold() { $K fn-down sleep:$1 fn-up; }
pastes() { [[ -n "${UTTER_LOG:-}" ]] && grep -c '^\[paste\]' "$UTTER_LOG" || print -1; }
nothing_pasted() { # nothing_pasted <paste count before>
  if [[ -n "${UTTER_LOG:-}" ]]; then (( $(pastes) == $1 )); else [[ -z "$(doc)" ]]; fi
}
wait_rows() { # wait_rows <expected> [timeout s]
  local t=0; while (( $(rows) < $1 && t < ${2:-12}*4 )); do sleep 0.25; ((t++)); done; sleep 0.8
}

PASS=0; FAIL=0; RESULTS=()
check() { # check <id> <description> <condition>
  if eval "$3"; then PASS=$((PASS+1)); RESULTS+=("PASS  $1  $2"); print "PASS  $1  $2"
  else FAIL=$((FAIL+1)); RESULTS+=("FAIL  $1  $2"); print "FAIL  $1  $2"; fi
}
run() { [[ -z "$FILTER" || "$1" == *$FILTER* ]]; }

reset_settings
q "delete from snippets where id like 'e2e-%'; delete from dictionary_entries where id like 'e2e-%';"
q "insert into snippets (id, trigger, expansion, created_at, updated_at) values ('e2e-snip','my email','test@example.com',1,1)"
q "insert into dictionary_entries (id, word, misspelling, source, created_at, updated_at) values ('e2e-dict','TanStack Query','react query','manual',1,1)"

# --- F02 push to talk -------------------------------------------------------------
if run F02-H1; then
  use speech; new_doc; printf 'ORIGINAL-CLIP' | pbcopy; b=$(rows)
  hold 1.5; wait_rows $((b+1))
  d=$(doc)
  check F02-H1a "text typed into the test window" '[[ "$d" == *meeting* ]]'
  check F02-H1b "fillers and repeat removed" '[[ "$d" != *Um* && "$d" != *"the the"* ]]'
  check F02-H1c "backtrack 5 actually 6 -> 6" '[[ "$d" == *"at 6"* && "$d" != *5* ]]'
  check F02-H1d "snippet expanded" '[[ "$d" == *test@example.com* ]]'
  check F02-H1e "clipboard restored" '[[ "$(pbpaste)" == ORIGINAL-CLIP ]]'
  check F02-H1f "row saved: app name recorded, status pasted" '[[ "$(last app_name)" == UtterTestTarget && "$(last status)" == pasted ]]'
fi

if run F02-H2; then
  use speech; printf 'CHROME-CLIP' | pbcopy; b=$(rows)
  open -a "Google Chrome" "data:text/html,<title>utter-test</title><textarea autofocus style='position:fixed;inset:0;width:100vw;height:100vh'></textarea>"
  # a cold Chrome start can take longer than a fixed wait
  for i in {1..20}; do [[ "$(osascript -e 'tell application "System Events" to name of first process whose frontmost is true')" == "Google Chrome" ]] && break; sleep 0.5; done; sleep 1.5
  $K click sleep:0.5
  hold 1.5; wait_rows $((b+1))
  clip=$(pbpaste)
  $K cmd-a cmd-c sleep:0.4; d=$(pbpaste)
  check F02-H2a "text typed into a Chrome textarea" '[[ "$d" == *meeting* ]]'
  check F02-H2b "clipboard restored after Chrome paste" '[[ "$clip" == CHROME-CLIP ]]'
  osascript -e 'tell application "Google Chrome" to close (every tab of every window whose title is "utter-test")' 2>/dev/null
fi

if run F02-E1; then
  use speech; new_doc; b=$(rows); pc=$(pastes)
  hold 0.2; sleep 2.5
  check F02-E1 "tap under 0.5 s is ignored (no row, nothing pasted)" '(( $(rows) == b )) && nothing_pasted $pc'
fi

if run F02-E2; then
  use speech; new_doc; b=$(rows); pc=$(pastes)
  $K fn-down sleep:1 esc fn-up; sleep 2.5
  check F02-E2 "Esc cancels (no row, nothing pasted)" '(( $(rows) == b )) && nothing_pasted $pc'
fi

if run F02-E3; then
  use speech; new_doc; b=$(rows)
  $K fn-down sleep:1 left fn-up; sleep 2.5
  check F02-E3 "fn+arrow is a shortcut, not dictation (no row)" '(( $(rows) == b ))'
fi

if run F02-E4; then
  use silence; new_doc; b=$(rows); pc=$(pastes)
  hold 1.5; sleep 4
  check F02-E4 "silence: nothing pasted, no row" '(( $(rows) == b )) && nothing_pasted $pc'
fi

if run F02-E5; then
  use short; new_doc; b=$(rows)
  hold 1; wait_rows $((b+1))
  check F02-E5 "one-word clip is transcribed" '[[ "$(doc)" == *Yes* ]]'
fi

if run F02-E6; then
  use long; new_doc; b=$(rows)
  hold 1.5; wait_rows $((b+1)) 40
  d=$(doc)
  check F02-E6a "long clip pasted, first and last sentence present" '[[ "$d" == *"number "(1|one)" "* && "$d" == *"number "(6|six)" "* ]]'
  check F02-E6b "long clip: no words dropped (>= 130 of 138 words)" '(( $(print -r -- "$d" | wc -w) >= 130 ))'
fi

if run F02-E7; then
  use german; new_doc; b=$(rows)
  setting "languages='[\"de\"]'"
  hold 1.5; wait_rows $((b+1))
  d=$(doc)
  check F02-E7 "German with language set to de keeps umlauts" '[[ "$d" == *Grüße* || "$d" == *München* ]]'
  reset_settings
fi

if run F02-E8; then
  use german; new_doc; b=$(rows)
  setting "auto_detect_language=1"
  hold 1.5; wait_rows $((b+1))
  check F02-E8 "German with auto-detect is transcribed as German" '[[ "$(doc)" == *München* || "$(doc)" == *Treffen* ]]'
  reset_settings
fi

if run F02-E9; then
  use short; new_doc; b=$(rows)
  $K fn-down sleep:1 fn-up sleep:0.2 fn-down sleep:1 fn-up; wait_rows $((b+2)) 15
  check F02-E9 "fn pressed again while processing is not lost (2 rows)" '(( $(rows) == b + 2 ))'
fi

if run F02-E10; then
  use speech; b=$(rows)
  pkill -x UtterTestTarget 2>/dev/null; osascript -e 'tell application "Finder" to activate'; sleep 1.2
  hold 1.5; wait_rows $((b+1))
  check F02-E10a "no text field focused: saved as paste_failed" '[[ "$(last status)" == paste_failed && "$(last error)" == no_text_field ]]'
  new_doc; $K ctrlcmd-v; sleep 1.5
  check F02-E10b "no text field: ctrl+cmd+V recovers the text" '[[ "$(doc)" == *meeting* ]]'
fi

if run F02-E11; then
  use short; new_doc; b=$(rows)
  osascript -e "set the clipboard to (read (POSIX file \"$PWD/$FX/pixel.png\") as «class PNGf»)"
  hold 1; wait_rows $((b+1))
  info=$(osascript -e 'clipboard info' 2>/dev/null)
  check F02-E11 "image on the clipboard survives a dictation" '[[ "$info" == *PNGf* ]]'
fi

if run F02-E12; then
  use correction; new_doc; b=$(rows)
  hold 1.5; wait_rows $((b+1))
  check F02-E12 "dictionary correction applied" '[[ "$(doc)" == *"TanStack Query"* ]]'
fi

if run F02-E13; then
  use raw; new_doc; b=$(rows)
  setting "cleanup_level='none'"
  hold 1.5; wait_rows $((b+1))
  check F02-E13 "cleanup off keeps the raw words" '[[ "$(last final_text)" == "$(last raw_text)" ]]'
  reset_settings
fi

if run F02-N1; then
  use raw; new_doc; b=$(rows)
  setting "llm_provider='groq'"
  hold 1.5; wait_rows $((b+1))
  check F02-N1 "Groq selected without a key: local cleanup still pastes" '[[ "$(doc)" == *ship* && "$(last status)" == pasted ]]'
  reset_settings
fi

if run F02-N2; then
  use raw; new_doc; b=$(rows)
  setting "llm_provider='ollama', ollama_url='http://127.0.0.1:59999'"
  hold 1.5; wait_rows $((b+1))
  check F02-N2 "Ollama unreachable: local cleanup still pastes" '[[ "$(doc)" == *ship* ]]'
  setting "ollama_url='http://127.0.0.1:11434'"; reset_settings
fi

# --- F03 hands-free -----------------------------------------------------------------
if run F03-H1; then
  use speech; new_doc; b=$(rows)
  $K fn-down space fn-up sleep:1.5 fn-down fn-up; wait_rows $((b+1))
  check F03-H1 "fn+space locks, fn stops, text pasted" '[[ "$(doc)" == *meeting* ]]'
fi

if run F03-E1; then
  use speech; new_doc; b=$(rows); pc=$(pastes)
  $K fn-down space fn-up sleep:1.5 esc; sleep 2.5
  check F03-E1 "Esc cancels a hands-free recording" '(( $(rows) == b )) && nothing_pasted $pc'
fi

# --- F04 recovery ---------------------------------------------------------------------
if run F03-H2; then
  use speech; new_doc; b=$(rows)
  $K fn-down sleep:0.15 fn-up sleep:0.15 fn-down sleep:0.15 fn-up sleep:1.5 fn-down sleep:0.1 fn-up; wait_rows $((b+1))
  check F03-H2 "double-press fn locks hands-free, next press stops and pastes" '[[ "$(doc)" == *meeting* ]] && (( $(rows) == b + 1 ))'
fi

if run F03-H3; then
  use speech; new_doc; b=$(rows)
  $K fn-down space fn-up sleep:1.5 click-bar:41; wait_rows $((b+1))
  check F03-H3 "clicking Stop on the bar pastes into the app that had focus" '[[ "$(doc)" == *meeting* ]]'
fi

if run F03-E2; then
  use speech; new_doc; b=$(rows)
  $K fn-down sleep:1.5 esc fn-up sleep:1 click-bar:31; wait_rows $((b+1))
  check F03-E2 "Undo on the bar after Esc transcribes and pastes after all" '[[ "$(doc)" == *meeting* ]] && (( $(rows) == b + 1 ))'
fi

if run F04-H1; then
  new_doc; want=$(last final_text)
  $K ctrlcmd-v; sleep 1.5
  check F04-H1 "ctrl+cmd+V pastes the last dictation" '[[ -n "$want" && "$(doc)" == "$want"* ]]'
fi

if run F04-H2; then
  printf 'x' | pbcopy; want=$(last final_text)
  $K ctrlcmd-c; sleep 0.8
  check F04-H2 "ctrl+cmd+C copies the last dictation" '[[ "$(pbpaste)" == "$want" ]]'
fi

# --- F12 retention -------------------------------------------------------------------
if run F12-H1; then
  use short; new_doc; b=$(rows); u=$(q "select coalesce(sum(sessions),0) from usage_days")
  setting "storage_mode='never'"
  hold 1; sleep 4
  check F12-H1a "never-store: text pasted" '[[ "$(doc)" == *Yes* ]]'
  check F12-H1b "never-store: no new row" '(( $(rows) == b ))'
  check F12-H1c "never-store: usage still counted" '(( $(q "select coalesce(sum(sessions),0) from usage_days") > u ))'
  new_doc; $K ctrlcmd-v; sleep 1.5
  check F12-H1d "never-store: ctrl+cmd+V still works from memory" '[[ "$(doc)" == *Yes* ]]'
  reset_settings
fi

# --- cleanup --------------------------------------------------------------------------
q "delete from snippets where id like 'e2e-%'; delete from dictionary_entries where id like 'e2e-%';"
reset_settings
print "\n$PASS passed, $FAIL failed"
print -l $RESULTS > results.txt
exit $(( FAIL > 0 ))
