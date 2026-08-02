use serde_json::{Value, json};
use soma_infra::{HostInspectRequest, PortListRequest, PortProtocol, ServiceListRequest};
use soma_ops::OperationName;
use tokio_util::sync::CancellationToken;

use crate::runtime_params::{optional_str, u32_or};
use crate::runtime_result::{items, metrics, resource, status};
use crate::{ExecutionError, SynapseReadRuntime};

impl SynapseReadRuntime {
    pub(crate) async fn execute_host(
        &self,
        operation: &OperationName,
        parameters: &Value,
        cancellation: &CancellationToken,
    ) -> Result<Value, ExecutionError> {
        if operation.as_str() == "fleet.nodes" {
            return self.topology_items().await;
        }
        let host = self.resolve_host(parameters).await?;
        match operation.as_str() {
            "host.status" => Ok(status(
                "online",
                json!({
                    "host": host.id(),
                    "topology_revision": host.revision(),
                    "endpoint": host.endpoint()
                }),
            )),
            "host.info" => resource(
                self.ports
                    .host
                    .inspect(
                        &host,
                        HostInspectRequest::new(self.deadline()),
                        cancellation,
                    )
                    .await?,
            ),
            "host.uptime" => {
                let inspection = self
                    .ports
                    .host
                    .inspect(
                        &host,
                        HostInspectRequest::new(self.deadline()),
                        cancellation,
                    )
                    .await?;
                metrics(json!({
                    "host": inspection.host,
                    "topology_revision": inspection.topology_revision,
                    "uptime_seconds": inspection.uptime_seconds
                }))
            }
            "host.resources" => {
                let inspection = self
                    .ports
                    .host
                    .inspect(
                        &host,
                        HostInspectRequest::new(self.deadline()),
                        cancellation,
                    )
                    .await?;
                metrics(json!({
                    "host": inspection.host,
                    "topology_revision": inspection.topology_revision,
                    "memory": inspection.memory,
                    "load": inspection.load
                }))
            }
            "host.services" => {
                let mut request = ServiceListRequest::new(self.deadline());
                if let Some(service) = optional_str(parameters, "service")? {
                    request = request.with_service(service)?;
                }
                if let Some(state) = optional_str(parameters, "state")?
                    && state != "all"
                {
                    request = request.with_state(state)?;
                }
                let rows = self
                    .ports
                    .host_system
                    .services(&host, &request, cancellation)
                    .await?;
                let count = rows.len();
                items(rows, count, false)
            }
            "host.network" => {
                let rows = self
                    .ports
                    .host_system
                    .network(&host, self.deadline(), cancellation)
                    .await?;
                resource(json!({
                    "host": host.id(),
                    "topology_revision": host.revision(),
                    "interfaces": rows
                }))
            }
            "host.mounts" => {
                let rows = self
                    .ports
                    .host_system
                    .mounts(&host, self.deadline(), cancellation)
                    .await?;
                let count = rows.len();
                items(rows, count, false)
            }
            "host.ports" => {
                let mut request = PortListRequest::new(self.deadline());
                if let Some(protocol) = optional_str(parameters, "protocol")? {
                    request = request.with_protocol(match protocol {
                        "udp" => PortProtocol::Udp,
                        _ => PortProtocol::Tcp,
                    });
                }
                request = request.with_page(
                    u32_or(parameters, "offset", 0)?,
                    u32_or(parameters, "limit", 5_000)?,
                )?;
                let rows = self
                    .ports
                    .host_system
                    .ports(&host, &request, cancellation)
                    .await?;
                let count = rows.len();
                items(rows, count, false)
            }
            "host.doctor" => {
                let report = self
                    .ports
                    .host_system
                    .doctor(&host, self.deadline(), cancellation)
                    .await?;
                let checks = report
                    .checks
                    .into_iter()
                    .map(|check| {
                        json!({
                            "code": check.name,
                            "status": if check.ok { "ok" } else { "failed" },
                            "summary": check.summary
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(json!({
                    "overall": match report.overall.as_str() {
                        "ok" => "ok",
                        "failed" => "failed",
                        _ => "warning"
                    },
                    "checks": checks
                }))
            }
            _ => Err(ExecutionError::UnsupportedOperation(operation.clone())),
        }
    }
}
