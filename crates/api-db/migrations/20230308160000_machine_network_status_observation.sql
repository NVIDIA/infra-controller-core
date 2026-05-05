ALTER TABLE IF EXISTS machines
    -- Before nico-dpu-agent first runs the network status has not been observed yet,
    -- therefore it is NULL.
    ADD COLUMN network_status_observation jsonb NULL
;
