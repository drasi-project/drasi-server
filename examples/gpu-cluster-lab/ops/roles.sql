\getenv config_password CONFIG_PASSWORD
\getenv plan_password PLAN_PASSWORD
\getenv replication_password REPLICATION_PASSWORD
CREATE ROLE gpu_config LOGIN PASSWORD :'config_password';
CREATE ROLE gpu_plan LOGIN PASSWORD :'plan_password';
CREATE ROLE gpu_reader LOGIN REPLICATION PASSWORD :'replication_password';
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
