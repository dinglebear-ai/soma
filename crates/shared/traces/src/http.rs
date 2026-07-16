use std::fmt;

use ::http::{HeaderMap, HeaderValue};
use rmcp::model::Meta;

use crate::trace_context::{
    parse_traceparent_value, validate_baggage_value, validate_tracestate_value,
};
use crate::{
    TraceLimits, TraceParseError, TraceSummary, TraceTrust, BAGGAGE_KEY, TRACEPARENT_KEY,
    TRACESTATE_KEY,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HttpTracePolicy {
    pub trust: TraceTrust,
    pub limits: TraceLimits,
    pub include_baggage: bool,
}

impl Default for HttpTracePolicy {
    fn default() -> Self {
        Self {
            trust: TraceTrust::Untrusted,
            limits: TraceLimits::default(),
            include_baggage: false,
        }
    }
}

pub struct HttpTraceExtraction {
    pub meta: Meta,
    pub summary: TraceSummary,
}

impl fmt::Debug for HttpTraceExtraction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let meta_keys = [TRACEPARENT_KEY, TRACESTATE_KEY, BAGGAGE_KEY]
            .into_iter()
            .filter(|key| self.meta.get(*key).is_some())
            .collect::<Vec<_>>();
        f.debug_struct("HttpTraceExtraction")
            .field("meta_keys", &meta_keys)
            .field("summary", &self.summary)
            .finish()
    }
}

pub fn extract_http_trace(headers: &HeaderMap, policy: HttpTracePolicy) -> HttpTraceExtraction {
    let mut meta = Meta::new();
    let traceparent_value = match single_header_value(headers, TRACEPARENT_KEY) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return HttpTraceExtraction {
                meta,
                summary: TraceSummary::absent_with_trust(policy.trust),
            };
        }
        Err(error) => {
            let mut summary = TraceSummary::absent_with_trust(policy.trust);
            summary.record_invalid_reason(&error);
            return HttpTraceExtraction { meta, summary };
        }
    };

    let traceparent = match parse_traceparent_value(traceparent_value, policy.limits) {
        Ok(traceparent) => traceparent,
        Err(error) => {
            let mut summary = TraceSummary::absent_with_trust(policy.trust);
            summary.record_invalid_reason(&error);
            return HttpTraceExtraction { meta, summary };
        }
    };

    meta.set_traceparent(traceparent_value);
    let mut summary = TraceSummary::from_valid_traceparent(&traceparent, policy.trust);

    match bounded_join_headers(headers, TRACESTATE_KEY, policy.limits.max_tracestate_len) {
        Ok(Some(tracestate)) => match validate_tracestate_value(Some(&tracestate)) {
            Ok(()) => {
                meta.set_tracestate(tracestate);
                summary.set_has_tracestate_for_http();
            }
            Err(error) => summary.record_invalid_reason(&error),
        },
        Ok(None) => {}
        Err(error) => summary.record_invalid_reason(&error),
    }

    if policy.include_baggage {
        match bounded_join_headers(headers, BAGGAGE_KEY, policy.limits.max_baggage_len) {
            Ok(Some(baggage)) => {
                match validate_baggage_value(Some(&baggage), policy.limits.max_baggage_members) {
                    Ok(()) => {
                        summary.set_baggage_counts_for_http(Some(&baggage));
                        meta.set_baggage(baggage);
                    }
                    Err(error) => summary.record_invalid_reason(&error),
                }
            }
            Ok(None) => {}
            Err(error) => summary.record_invalid_reason(&error),
        }
    }

    HttpTraceExtraction { meta, summary }
}

fn single_header_value<'a>(
    headers: &'a HeaderMap,
    field: &'static str,
) -> Result<Option<&'a str>, TraceParseError> {
    let values = headers.get_all(field);
    match values.iter().take(2).count() {
        0 => Ok(None),
        1 => {
            let value = values
                .iter()
                .next()
                .expect("count confirmed one header value");
            header_to_str(value, field).map(Some)
        }
        _ => Err(TraceParseError::MultipleHeaderValues { field }),
    }
}

fn bounded_join_headers(
    headers: &HeaderMap,
    field: &'static str,
    max: usize,
) -> Result<Option<String>, TraceParseError> {
    let mut joined = String::new();
    let mut saw_value = false;
    for value in headers.get_all(field).iter() {
        let value = header_to_str(value, field)?;
        let separator_len = usize::from(saw_value);
        let required_len = joined.len() + separator_len + value.len();
        if required_len > max {
            return Err(TraceParseError::ValueTooLong {
                field,
                actual: max + 1,
                max,
            });
        }
        if saw_value {
            joined.push(',');
        }
        joined.push_str(value);
        saw_value = true;
    }
    Ok(saw_value.then_some(joined))
}

fn header_to_str<'a>(
    value: &'a HeaderValue,
    field: &'static str,
) -> Result<&'a str, TraceParseError> {
    value
        .to_str()
        .map_err(|_| TraceParseError::InvalidHeaderValue { field })
}
