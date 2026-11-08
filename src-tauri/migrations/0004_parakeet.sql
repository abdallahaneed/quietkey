-- Parakeet v3 joins the models and becomes the default. SQLite cannot change a CHECK, so the column is rebuilt
-- and each install keeps the model it had.
alter table settings add column model_old text;
update settings set model_old = model;
alter table settings drop column model;
alter table settings add column model text not null default 'parakeet-v3'
  check (model in ('parakeet-v3', 'base', 'small', 'large-v3-turbo'));
update settings set model = model_old;
alter table settings drop column model_old;
