-- Cohere Transcribe Arabic joins the models (the CHECK is rebuilt as in 0004), and history entries can be starred.
alter table settings add column model_old text;
update settings set model_old = model;
alter table settings drop column model;
alter table settings add column model text not null default 'parakeet-v3'
  check (model in ('parakeet-v3', 'cohere-ar', 'base', 'small', 'large-v3-turbo'));
update settings set model = model_old;
alter table settings drop column model_old;

-- Starred entries survive "delete after 24 hours".
alter table transcripts add column favorite integer not null default 0 check (favorite in (0,1));
