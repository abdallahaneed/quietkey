-- Local SQLite database: ~/Library/Application Support/dev.abdallah.utter/app.db
-- Single local user, so no owner columns. Access control is the macOS file permission on the app's own folder.
-- ids: UUIDv7 text made in Rust (sortable by time). Times: unix epoch milliseconds, UTC, integer.


-- One row only (id = 1). Typed columns instead of key/value so bad values fail at write time.
create table settings (
  id                   integer primary key check (id = 1),
  shortcuts            text    not null default '{"push_to_talk":[["Fn"]],"hands_free":[["Fn","Space"]],"cancel":[["Escape"]],"paste_last":[["Ctrl","Cmd","V"]],"copy_last":[["Ctrl","Cmd","C"]]}'
                               check (json_valid(shortcuts)),
  mic_id               text,                                  -- null = auto-detect
  mic_ranking          text    not null default '[]' check (json_valid(mic_ranking)),
  languages            text    not null default '["en"]' check (json_valid(languages)),
  auto_detect_language integer not null default 0 check (auto_detect_language in (0,1)),
  theme                text    not null default 'system' check (theme in ('light','dark','system')),
  launch_at_login      integer not null default 1 check (launch_at_login in (0,1)),
  show_bar             integer not null default 1 check (show_bar in (0,1)),
  bar_edge             text    not null default 'bottom' check (bar_edge in ('bottom','left','right')),
  show_in_dock         integer not null default 0 check (show_in_dock in (0,1)),
  mute_audio           integer not null default 0 check (mute_audio in (0,1)),
  sounds               integer not null default 1 check (sounds in (0,1)),
  storage_mode         text    not null default 'store' check (storage_mode in ('store','auto_delete_24h','never')),
  cleanup_level        text    not null default 'light' check (cleanup_level in ('none','light','medium')),
  llm_provider         text    not null default 'none' check (llm_provider in ('none','groq','ollama')), -- none = offline rules only
  groq_model           text    not null default 'llama-3.3-70b-versatile',
  ollama_model         text    not null default 'llama3.2:3b',
  ollama_url           text    not null default 'http://127.0.0.1:11434',
  press_enter_enabled  integer not null default 0 check (press_enter_enabled in (0,1)),
  command_mode_enabled integer not null default 0 check (command_mode_enabled in (0,1)),
  onboarding_done      integer not null default 0 check (onboarding_done in (0,1)),
  updated_at           integer not null default (unixepoch('subsec') * 1000)
) strict;
insert into settings (id) values (1);

create table transcripts (
  id            text    primary key,
  created_at    integer not null,
  raw_text      text    not null,                 -- Whisper output, before cleanup
  final_text    text    not null,                 -- what was pasted
  app_bundle_id text,                             -- frontmost app at key-up
  app_name      text,
  language      text,
  duration_ms   integer not null check (duration_ms >= 0),
  word_count    integer not null check (word_count >= 0),
  audio_path    text,                             -- relative to app folder; null after audio purge
  cleanup_level text    not null check (cleanup_level in ('none','light','medium')),
  tone          text    check (tone in ('formal','casual','very_casual','excited')),
  status        text    not null check (status in ('pasted','paste_failed','cancelled','error')),
  error         text,
  updated_at    integer not null
) strict;
create index transcripts_created_at on transcripts (created_at desc);
create index transcripts_audio on transcripts (created_at) where audio_path is not null;

create table dictionary_entries (
  id          text    primary key,
  word        text    not null check (length(trim(word)) between 1 and 60),
  misspelling text    check (misspelling is null or length(trim(misspelling)) between 1 and 60),
  source      text    not null default 'manual' check (source in ('manual','auto','import')),
  created_at  integer not null,
  updated_at  integer not null
) strict;
-- "Duplicates blocked": same word + same misspelling, ignoring case.
create unique index dictionary_unique on dictionary_entries (lower(word), lower(coalesce(misspelling, '')));

create table snippets (
  id         text    primary key,
  trigger    text    not null check (length(trim(trigger)) between 1 and 60),
  expansion  text    not null check (length(expansion) between 1 and 4000),
  created_at integer not null,
  updated_at integer not null
) strict;
-- Matching ignores capitalization, so two triggers differing only in case would be ambiguous.
create unique index snippets_trigger on snippets (lower(trim(trigger)));

create table style_settings (
  category   text    primary key check (category in ('personal','work','email','other')),
  tone       text    not null default 'formal' check (tone in ('formal','casual','very_casual','excited')),
  updated_at integer not null default (unixepoch('subsec') * 1000),
  -- very_casual is Personal only; excited is not offered for Personal.
  check (not (tone = 'very_casual' and category <> 'personal')),
  check (not (tone = 'excited' and category = 'personal'))
) strict;
insert into style_settings (category) values ('personal'), ('work'), ('email'), ('other');

-- Seeded with known apps (Messages, WhatsApp -> personal; Slack, Teams -> work; Mail, Outlook -> email).
-- Unknown apps fall back to 'other'. User overrides set user_set = 1 so a reseed never clobbers them.
create table app_categories (
  bundle_id  text    primary key,
  app_name   text,
  category   text    not null references style_settings (category) on delete restrict,
  user_set   integer not null default 0 check (user_set in (0,1)),
  updated_at integer not null
) strict;
create index app_categories_category on app_categories (category);

-- Kept separately from transcripts so stats survive "never store" mode and history deletion.
create table usage_days (
  day          text    primary key check (day glob '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'), -- local date
  words        integer not null default 0 check (words >= 0),
  sessions     integer not null default 0 check (sessions >= 0),
  speaking_ms  integer not null default 0 check (speaking_ms >= 0),   -- wpm = words / (speaking_ms / 60000)
  fixes        integer not null default 0 check (fixes >= 0)
) strict;
