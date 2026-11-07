-- F6: the Whisper model the user picked. Files live in <app data>/models, see stt::MODELS.
alter table settings add column model text not null default 'large-v3-turbo'
  check (model in ('base', 'small', 'large-v3-turbo'));
