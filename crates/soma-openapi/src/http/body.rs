use crate::error::OpenApiError;

pub(crate) async fn collect_spec_capped(
    response: reqwest::Response,
    cap: usize,
    label: &str,
) -> Result<String, OpenApiError> {
    collect_capped(response, cap, label, true).await
}

pub(crate) async fn collect_response_capped(
    response: reqwest::Response,
    cap: usize,
    label: &str,
) -> Result<String, OpenApiError> {
    collect_capped(response, cap, label, false).await
}

async fn collect_capped(
    mut response: reqwest::Response,
    cap: usize,
    label: &str,
    spec_body: bool,
) -> Result<String, OpenApiError> {
    let mut buffer = Vec::new();
    while let Some(bytes) = response
        .chunk()
        .await
        .map_err(|_| OpenApiError::UpstreamRequest {
            label: label.to_string(),
        })?
    {
        if buffer.len() + bytes.len() > cap {
            return if spec_body {
                Err(OpenApiError::SpecTooLarge {
                    label: label.to_string(),
                })
            } else {
                Err(OpenApiError::UpstreamRequest {
                    label: label.to_string(),
                })
            };
        }
        buffer.extend_from_slice(&bytes);
    }
    String::from_utf8(buffer).map_err(|_| {
        if spec_body {
            OpenApiError::SpecParse {
                label: label.to_string(),
            }
        } else {
            OpenApiError::UpstreamRequest {
                label: label.to_string(),
            }
        }
    })
}
