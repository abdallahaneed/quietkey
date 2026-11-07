#!/bin/zsh
# End-to-end dictation test without a microphone or a human.
# Prereqs: the terminal running this has Accessibility + Input Monitoring; app started with
#   QUIETKEY_TEST_WAV=$PWD/scripts/e2e/speech.wav npm run tauri dev
# macOS "Press fn key to" should be "Do Nothing", or Apple Dictation types over the result.
set -euo pipefail
cd "${0:A:h}"
DB="$HOME/Library/Application Support/dev.abdallah.quietkey/app.db"
[[ keys -nt keys.swift ]] || swiftc -O keys.swift -o keys
[[ -f speech.wav ]] || { say -o speech.aiff "Um, so the the meeting is at 5, actually 6 p m. Please send the notes to my email. Thanks a lot."; afconvert -f WAVE -d LEI16@16000 -c 1 speech.aiff speech.wav; rm speech.aiff; }

sqlite3 "$DB" "insert or ignore into snippets (id, trigger, expansion, created_at, updated_at) values ('e2e-snip','my email','test@example.com',1,1)"
printf 'ORIGINAL-CLIP' | pbcopy
[[ -x QuietkeyTestTarget ]] || swiftc -O target.swift -o QuietkeyTestTarget
OUT=$(mktemp -t quietkey-target)
./QuietkeyTestTarget "$OUT" & sleep 1.5
trap 'pkill -x QuietkeyTestTarget 2>/dev/null; rm -f "$OUT"' EXIT
sleep 1.5
before=$(sqlite3 "$DB" "select count(*) from transcripts")
./keys fn-down sleep:1.5 fn-up sleep:4
doc=$(cat "$OUT")
after=$(sqlite3 "$DB" "select count(*) from transcripts")
sqlite3 "$DB" "delete from snippets where id='e2e-snip'"

fail=0
check() { if eval "$2"; then echo "PASS  $1"; else echo "FAIL  $1"; fail=1; fi; }
check "text inserted into the test window"           '[[ "$doc" == *"meeting"* ]]'
check "filler and repeat removed"             '[[ "$doc" != *"Um"* && "$doc" != *"the the"* ]]'
check "backtrack: 5 actually 6 -> 6"          '[[ "$doc" == *"at 6"* && "$doc" != *"5"* ]]'
check "snippet expanded"                      '[[ "$doc" == *"test@example.com"* ]]'
check "clipboard restored"                    '[[ "$(pbpaste)" == "ORIGINAL-CLIP" ]]'
check "history row saved"                     '(( after == before + 1 ))'
exit $fail
