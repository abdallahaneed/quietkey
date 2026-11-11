#!/bin/zsh
# Live text tests: the preview types into the field while fn is held, and the final text replaces it exactly.
# Start the app with the WAV played in at real speed:
#   QUIETKEY_TEST_WAV=$PWD/scripts/e2e/fixtures/current.wav QUIETKEY_TEST_LIVE=1 npm run tauri dev
# Usage: scripts/e2e/live.sh [case-id-filter]   (run suite.sh once first: it makes the fixtures)
setopt NULL_GLOB
zmodload zsh/datetime
cd "${0:A:h}"
FILTER="${1:-}"
DB="$HOME/Library/Application Support/dev.abdallah.quietkey/app.db"
FX=fixtures
K=./keys
OUT=$(mktemp -t quietkey-target)
MODEL=$(sqlite3 "$DB" "select model from settings where id = 1")

use() { ln -sf "${PWD}/$FX/$1.wav" $FX/current.wav; }
secs() { python3 -c "import wave,sys; w=wave.open(sys.argv[1]); print(w.getnframes()/w.getframerate())" $FX/$1.wav; }
q() { sqlite3 "$DB" "$1"; }
rows() { q "select count(*) from transcripts"; }
last() { q "select $1 from transcripts order by created_at desc, id desc limit 1"; }
front() { osascript -e 'tell application "System Events" to name of first process whose frontmost is true'; }
# macOS sometimes leaves a freshly started window behind Terminal; then the keys would go to Terminal. Insist.
new_doc() {
  pkill -x QuietkeyTestTarget 2>/dev/null; sleep 0.3; ./QuietkeyTestTarget "$OUT" & sleep 1.5
  for i in {1..10}; do [[ "$(front)" == QuietkeyTestTarget ]] && return
    osascript -e 'tell application "System Events" to set frontmost of process "QuietkeyTestTarget" to true'; sleep 0.5; done
  print "      test window never came to the front"; exit 2
}
doc() { cat "$OUT" 2>/dev/null; }
wait_rows() { local t=0; while (( $(rows) < $1 && t < ${2:-40}*4 )); do sleep 0.25; ((t++)); done; sleep 1; }
# Same backup as suite.sh; if not exists: a run that was killed left its backup, which still holds the real settings
q "create table if not exists e2e_settings_backup as select * from settings"
trap 'pkill -x QuietkeyTestTarget 2>/dev/null; rm -f "$OUT"; q "replace into settings select * from e2e_settings_backup; drop table e2e_settings_backup"' EXIT
q "update settings set cleanup_level='light', llm_provider='none', storage_mode='store' where id = 1"

PASS=0; FAIL=0
DISTURBED=0
# Another app came to the front during the case (a click, an app bringing itself forward): the result says nothing.
check() {
  if [[ -n "$OTHER" ]]; then DISTURBED=$((DISTURBED+1)); print "SKIP  $MODEL  $1  $2 (disturbed: $OTHER came to the front)"
  elif eval "$3"; then PASS=$((PASS+1)); print "PASS  $MODEL  $1  $2"; else FAIL=$((FAIL+1)); print "FAIL  $MODEL  $1  $2"; fi
}
# release_fn: lets go of fn and notes if the test window was not in front at that moment
release_fn() { local f=$(front); OTHER=""; [[ "$f" == QuietkeyTestTarget ]] || OTHER=$f; $K fn-up; }
run() { [[ -z "$FILTER" || "$1" == *$FILTER* ]]; }

# L1 one dictation: words appear before release, field equals the saved text afterwards (nothing doubled or left over)
if run L1; then
  use speech; new_doc; b=$(rows); len=$(secs speech)
  $K fn-down; sleep $((len / 2)); mid=$(doc); sleep $((len / 2 + 1.5)); end=$(doc); t0=$EPOCHREALTIME; release_fn
  wait_rows $((b+1)); d=$(doc); f=$(last final_text)
  print "      mid: ${mid}\n      end: ${end}\n      doc: ${d}\n      db:  ${f}\n      release->row ~$(( EPOCHREALTIME - t0 )) s"
  check L1a "live text in the field while talking" '[[ -n "$end" ]]'
  check L1b "live text grew" '[[ -z "$mid" ]] || (( ${#end} > ${#mid} ))'
  check L1c "field equals the saved text after release" '[[ "$d" == "$f" && -n "$f" ]]'
fi

# L2 esc while talking removes the live text and pastes nothing
if run L2; then
  use speech; new_doc; b=$(rows)
  $K fn-down; sleep 4; mid=$(doc); f=$(front); OTHER=""; [[ "$f" == QuietkeyTestTarget ]] || OTHER=$f; $K esc; sleep 0.3; $K fn-up; sleep 2; d=$(doc)
  print "      mid: ${mid}\n      doc: ${d}"
  check L2a "live text was there before esc" '[[ -n "$mid" ]]'
  check L2b "field empty after esc" '[[ -z "$d" ]]'
  check L2c "no row saved" '(( $(rows) == b ))'
fi

# L3 two dictations back to back: the second starts while the first is still being transcribed
if run L3; then
  use german; new_doc; b=$(rows); len=$(secs german)
  $K fn-down; sleep $((len + 0.5)); release_fn; o1=$OTHER; sleep 0.3
  $K fn-down; sleep $((len + 0.5)); release_fn; OTHER=${OTHER:-$o1}
  wait_rows $((b+2)); d=$(doc)
  f2=$(q "select final_text from transcripts order by created_at desc, id desc limit 1 offset 1"); f1=$(last final_text)
  print "      doc: ${d}\n      db1: ${f2}\n      db2: ${f1}"
  check L3a "both texts in the field, in order, nothing lost or doubled" '[[ "$d" == "$f2$f1" || "$d" == "$f2 $f1" ]]'
fi

# L4 a minute of speech: final text still exact, and how long the release waits
if run L4; then
  use long; new_doc; b=$(rows); len=$(secs long); printf 'LIVE-CLIP' | pbcopy
  $K fn-down; sleep $((len + 1)); end=$(doc); t0=$EPOCHREALTIME; release_fn
  wait_rows $((b+1)) 90; dt=$(( EPOCHREALTIME - t0 )); d=$(doc); f=$(last final_text)
  print "      live chars at release: ${#end}, final chars: ${#f}, release->row ~${dt} s"
  [[ "$d" == "$f" ]] || print "      doc: ${d}\n      db:  ${f}"
  check L4a "field equals the saved text (long)" '[[ "$d" == "$f" && -n "$f" ]]'
  check L4b "clipboard untouched and never pasted" '[[ "$(pbpaste)" == LIVE-CLIP && "$d" != *LIVE-CLIP* ]]'
fi

# L5 switching apps mid-dictation: the first app keeps its live text untouched, the second gets the full text once
if run L5; then
  use speech; new_doc; b=$(rows); len=$(secs speech); OUT2=$(mktemp -t quietkey-target2)
  $K fn-down; sleep $((len / 2)); ./QuietkeyTestTarget "$OUT2" & sleep 1; at_switch=$(doc)
  sleep $((len / 2 + 1)); f=$(front); OTHER=""; [[ "$f" == QuietkeyTestTarget ]] || OTHER=$f; $K fn-up
  wait_rows $((b+1)); d1=$(doc); d2=$(cat "$OUT2"); f=$(last final_text)
  print "      first at switch: ${at_switch}\n      first after:     ${d1}\n      second:          ${d2}\n      db:              ${f}"
  check L5a "second app got exactly the saved text" '[[ "$d2" == "$f" && -n "$f" ]]'
  check L5b "nothing deleted in the first app" '[[ "$d1" == "$at_switch"* ]]'
  rm -f "$OUT2"
fi

# L6 room noise only: nothing typed live, nothing pasted
if run L6; then
  use noise; new_doc; len=$(secs noise)
  $K fn-down; sleep $((len + 0.5)); mid=$(doc); release_fn; sleep 3; d=$(doc)
  print "      while held: ${mid}\n      after:      ${d}"
  check L6a "no live text from noise" '[[ -z "$mid" ]]'
  check L6b "field empty after release" '[[ -z "$d" ]]'
fi

# L7 live text switched off in Settings: nothing while talking, the whole text pasted at release
if run L7; then
  q "update settings set live_text=0 where id = 1"
  use speech; new_doc; b=$(rows); len=$(secs speech)
  $K fn-down; sleep $((len + 1)); mid=$(doc); release_fn
  wait_rows $((b+1)); d=$(doc); f=$(last final_text)
  print "      while held: ${mid}\n      doc: ${d}"
  check L7a "live text off: nothing typed while talking" '[[ -z "$mid" ]]'
  check L7b "live text off: full text pasted at release" '[[ "$d" == "$f" && -n "$f" ]]'
  q "update settings set live_text=1 where id = 1"
fi

# L8 "stop live text after 30 seconds": the live words stop growing after 30 s, the final text is still complete
if run L8; then
  q "update settings set live_text_limit=1 where id = 1"
  use long; new_doc; b=$(rows); len=$(secs long)
  $K fn-down; sleep 33; at33=$(doc); sleep $((len - 33 + 1)); end=$(doc); t0=$EPOCHREALTIME; release_fn
  wait_rows $((b+1)) 90; dt=$(( EPOCHREALTIME - t0 )); d=$(doc); f=$(last final_text)
  print "      live chars at 33 s: ${#at33}, at release: ${#end}, final: ${#f}, release->row ~${dt} s"
  check L8a "live text stopped growing after 30 s" '[[ -n "$at33" && "$end" == "$at33" ]]'
  check L8b "final text complete and exact" '[[ "$d" == "$f" && ${#f} -gt ${#end} ]]'
  q "update settings set live_text_limit=0 where id = 1"
fi

# L9 Chrome: live text and the final text in a browser textarea (keys go straight to Chrome)
if run L9; then
  use speech; b=$(rows); len=$(secs speech); printf 'L9-CLIP' | pbcopy
  pkill -x QuietkeyTestTarget 2>/dev/null
  open -a "Google Chrome" "data:text/html,<title>quietkey-live</title><textarea autofocus style='position:fixed;inset:0;width:100vw;height:100vh'></textarea>"
  for i in {1..20}; do [[ "$(front)" == "Google Chrome" ]] && break; sleep 0.5; done; sleep 1.5
  $K click sleep:0.5
  lives() { grep -c '^\[live\]' "${QUIETKEY_LOG:-/dev/null}"; }
  l0=$(lives)
  $K fn-down; sleep $((len + 1)); l1=$(lives); f0=$(front); $K fn-up
  OTHER=""; [[ "$f0" == "Google Chrome" ]] || OTHER=$f0
  wait_rows $((b+1)); $K cmd-a cmd-c sleep:0.4; d=$(pbpaste); f=$(last final_text)
  print "      live updates sent: $((l1 - l0))\n      doc: ${d}\n      db:  ${f}"
  check L9a "Chrome: live text while talking" '(( l1 > l0 ))'
  check L9b "Chrome: field equals the saved text" '[[ "$d" == "$f" && -n "$f" ]]'
  osascript -e 'tell application "Google Chrome" to close (every tab of every window whose title is "quietkey-live")' 2>/dev/null
fi

print "\n$MODEL: $PASS passed, $FAIL failed, $DISTURBED disturbed"
exit $(( FAIL > 0 ))
