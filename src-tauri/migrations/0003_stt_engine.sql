-- Where speech is turned into text: 'local' (Whisper on this Mac) or 'groq' (Groq's Whisper with the user's own key).
alter table settings add column stt_engine text not null default 'local'
  check (stt_engine in ('local', 'groq'));
