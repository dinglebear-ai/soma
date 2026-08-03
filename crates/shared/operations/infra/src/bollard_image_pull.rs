use futures_util::StreamExt;
use serde_json::Value;
use soma_fleet::{HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, ProgressEvent, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{
    BollardReadClient, ImagePullMutator, ImagePullProgressFrame, ImagePullReceipt,
    ImagePullRequest, InfraError, MutationFailure, MutationProgressReporter, MutationResult,
};

#[async_trait::async_trait]
impl ImagePullMutator for BollardReadClient {
    async fn pull_image(
        &self,
        host: &HostRecord,
        request: &ImagePullRequest,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> MutationResult<ImagePullReceipt> {
        self.validate_host(host)
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        ensure_not_expired(request.deadline(), cancellation)?;
        let (from_image, tag) = split_image_reference(request.image());
        let options = bollard::query_parameters::CreateImageOptions {
            from_image: Some(from_image),
            tag,
            ..Default::default()
        };
        let mut stream = self.docker().create_image(Some(options), None, None);
        let mut receipt = ImagePullReceipt {
            host: host.id().clone(),
            topology_revision: TopologyRevision::clone(host.revision()),
            image: request.image().to_owned(),
            send_state: MutationSendState::Sent,
            total_events: 0,
            progress: Vec::new(),
            progress_truncated: false,
            progress_delivery_errors: Vec::new(),
        };
        let mut sequence = 0_u64;
        loop {
            let remaining = remaining_duration(request.deadline())?;
            let next = tokio::select! {
                () = cancellation.cancelled() => {
                    return Err(MutationFailure::new(
                        MutationSendState::Unknown,
                        soma_fleet::FleetError::Cancelled.into(),
                    ));
                }
                () = tokio::time::sleep(remaining) => {
                    return Err(MutationFailure::new(
                        MutationSendState::Unknown,
                        soma_fleet::FleetError::DeadlineExceeded.into(),
                    ));
                }
                next = stream.next() => next,
            };
            let Some(item) = next else {
                break;
            };
            let info = item.map_err(|error| {
                MutationFailure::new(
                    MutationSendState::Unknown,
                    InfraError::Docker(error.to_string()),
                )
            })?;
            sequence = sequence.saturating_add(1);
            let value = serde_json::to_value(&info).map_err(|error| {
                MutationFailure::new(
                    MutationSendState::Sent,
                    InfraError::Parse {
                        domain: "image-pull",
                        message: error.to_string(),
                    },
                )
            })?;
            let frame = progress_frame(sequence, &value);
            if let Some(error) = frame.error.clone() {
                receipt.retain_frame(frame);
                return Err(MutationFailure::new(
                    MutationSendState::Sent,
                    InfraError::Docker(error),
                ));
            }
            if let Ok(event) = canonical_progress_event(request, &frame)
                && let Err(error) = progress.report(&event)
            {
                receipt.retain_delivery_error(error);
            }
            receipt.retain_frame(frame);
        }
        Ok(receipt)
    }
}

fn ensure_not_expired(deadline: Timestamp, cancellation: &CancellationToken) -> MutationResult<()> {
    if cancellation.is_cancelled() {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::Cancelled.into(),
        ));
    }
    if Timestamp::now() >= deadline {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::DeadlineExceeded.into(),
        ));
    }
    Ok(())
}

fn remaining_duration(deadline: Timestamp) -> MutationResult<std::time::Duration> {
    let remaining = deadline
        .unix_millis()
        .saturating_sub(Timestamp::now().unix_millis());
    if remaining <= 0 {
        Err(MutationFailure::new(
            MutationSendState::Unknown,
            soma_fleet::FleetError::DeadlineExceeded.into(),
        ))
    } else {
        Ok(std::time::Duration::from_millis(remaining as u64))
    }
}

fn split_image_reference(image: &str) -> (String, Option<String>) {
    if image.contains('@') {
        return (image.to_owned(), None);
    }
    match image.rsplit_once(':') {
        Some((repo, tag)) if !tag.contains('/') && !tag.is_empty() => {
            (repo.to_owned(), Some(tag.to_owned()))
        }
        _ => (image.to_owned(), None),
    }
}

fn progress_frame(sequence: u64, value: &Value) -> ImagePullProgressFrame {
    let detail = value
        .get("progress_detail")
        .or_else(|| value.get("progressDetail"));
    ImagePullProgressFrame {
        sequence,
        status: text(value, &["status", "Status"]),
        id: text(value, &["id", "ID", "Id"]),
        current: detail.and_then(|value| unsigned(value, &["current", "Current"])),
        total: detail.and_then(|value| unsigned(value, &["total", "Total"])),
        message: text(value, &["progress", "Progress"]),
        error: text(value, &["error", "Error"]).or_else(|| {
            value
                .get("error_detail")
                .or_else(|| value.get("errorDetail"))
                .and_then(|detail| text(detail, &["message", "Message"]))
        }),
    }
}

fn canonical_progress_event(
    request: &ImagePullRequest,
    frame: &ImagePullProgressFrame,
) -> Result<ProgressEvent, soma_ops::ProgressError> {
    let mut event = ProgressEvent::new(
        request.operation_id().clone(),
        request.operation().clone(),
        frame.sequence,
        Timestamp::now(),
        "pull",
    )?;
    if let Some(current) = frame.current
        && frame
            .total
            .is_none_or(|total| total > 0 && current <= total)
    {
        event = event.with_amount(current, frame.total, Some("bytes"))?;
    }
    let message = [
        frame.status.as_deref(),
        frame.id.as_deref(),
        frame.message.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    if !message.is_empty() {
        event = event.with_message(bounded_message(&message))?;
    }
    Ok(event)
}

fn bounded_message(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(1_024)
        .collect()
}

fn text(value: &Value, names: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    names
        .iter()
        .find_map(|name| object.get(*name))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn unsigned(value: &Value, names: &[&str]) -> Option<u64> {
    let object = value.as_object()?;
    names
        .iter()
        .find_map(|name| object.get(*name))
        .and_then(Value::as_u64)
}

#[cfg(test)]
#[path = "bollard_image_pull_tests.rs"]
mod tests;
