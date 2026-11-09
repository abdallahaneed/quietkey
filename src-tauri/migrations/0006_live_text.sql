-- Live text while talking (0.2.1): on by default; optionally stop updating it after 30 seconds of a dictation.
alter table settings add column live_text integer not null default 1 check (live_text in (0,1));
alter table settings add column live_text_limit integer not null default 0 check (live_text_limit in (0,1));
