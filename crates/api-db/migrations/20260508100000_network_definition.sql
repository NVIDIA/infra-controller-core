-- Snapshot of the `NetworkDefinition` used to seed each network segment.
-- Written once, when a network is first seeded.
CREATE TABLE network_def (
    name        TEXT PRIMARY KEY,
    definition  JSONB NOT NULL,
    seeded_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
