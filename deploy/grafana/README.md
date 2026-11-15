# Grafana and Prometheus (T10.11)

The control plane serves Prometheus text at `/metrics`.

* Without `[metrics]` in `control.toml` it shows the process's own HTTP metrics (as before).
* With a token (`metrics.token_file`, mode 0600) it also lists every node and account
  (`dhpanel_node_*`, `dhpanel_tenant_*`, `dhpanel_tenants`, `dhpanel_operations`) and refuses a
  request without `Authorization: Bearer <token>`. Account names are labels: keep the
  endpoint on a private network or behind the front proxy's allow-list.

Prometheus:

```yaml
scrape_configs:
  - job_name: dhpanel
    scheme: https
    metrics_path: /metrics
    authorization:
      credentials_file: /etc/prometheus/dhpanel.token
    scrape_interval: 60s          # the nodes are read once a minute
    static_configs:
      - targets: ["panel.example.com"]
```

Grafana: Dashboards > New > Import > upload `dhpanel-fleet.json`, choose the Prometheus data
source. The nodes' own agents have no HTTP port: their numbers reach `/metrics` through the
control plane, one reading a minute.
