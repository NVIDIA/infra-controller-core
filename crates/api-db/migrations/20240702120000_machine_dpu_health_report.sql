-- Adda field to store the health report receive by nico-dpu-agent
ALTER TABLE machines ADD COLUMN dpu_agent_health_report jsonb;
