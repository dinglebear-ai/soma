use std::{collections::BTreeSet, error::Error, fmt};

use rmcp::model::Meta;
use serde_json::Value;

pub const TRACEPARENT_KEY: &str = "traceparent";
pub const TRACESTATE_KEY: &str = "tracestate";
pub const BAGGAGE_KEY: &str = "baggage";

const TRACEPARENT_V00_LEN: usize = 55;
const TRACEPARENT_VERSION_END: usize = 2;
const TRACEPARENT_TRACE_ID_START: usize = 3;
const TRACEPARENT_TRACE_ID_END: usize = 35;
const TRACEPARENT_SPAN_ID_START: usize = 36;
const TRACEPARENT_SPAN_ID_END: usize = 52;
const TRACEPARENT_FLAGS_START: usize = 53;
const TRACEPARENT_FLAGS_END: usize = 55;
const TRACEPARENT_NEXT_SEPARATOR: usize = 55;
const MAX_TRACESTATE_MEMBERS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceTrust {
    Untrusted,
    Trusted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceLimits {
    pub max_traceparent_len: usize,
    pub max_tracestate_len: usize,
    pub max_baggage_len: usize,
    pub max_baggage_members: usize,
}

impl Default for TraceLimits {
    fn default() -> Self {
        Self {
            max_traceparent_len: 512,
            max_tracestate_len: 512,
            max_baggage_len: 8 * 1024,
            max_baggage_members: 64,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct TraceParent {
    trace_id: String,
    span_id: String,
    sampled: bool,
}

impl TraceParent {
    #[cfg(test)]
    fn parse(value: &str) -> Result<Self, TraceParseError> {
        let limits = TraceLimits::default();
        if value.len() > limits.max_traceparent_len {
            return Err(TraceParseError::ValueTooLong {
                field: TRACEPARENT_KEY,
                actual: value.len(),
                max: limits.max_traceparent_len,
            });
        }
        parse_traceparent(value)
    }

    #[cfg(test)]
    fn trace_id(&self) -> &str {
        &self.trace_id
    }

    #[cfg(test)]
    fn span_id(&self) -> &str {
        &self.span_id
    }

    fn sampled(&self) -> bool {
        self.sampled
    }

    fn trace_id_short(&self) -> &str {
        &self.trace_id[..8]
    }

    fn span_id_short(&self) -> &str {
        &self.span_id[..8]
    }
}

impl fmt::Debug for TraceParent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TraceParent")
            .field("trace_id", &self.trace_id_short())
            .field("span_id", &self.span_id_short())
            .field("sampled", &self.sampled)
            .finish()
    }
}

#[cfg(test)]
#[derive(Clone, PartialEq, Eq)]
struct TraceContext {
    traceparent: TraceParent,
    tracestate: Option<String>,
    baggage: Option<String>,
    trust: TraceTrust,
}

#[cfg(test)]
impl TraceContext {
    fn from_meta(meta: &Meta, trust: TraceTrust) -> Result<Option<Self>, TraceParseError> {
        Self::from_meta_with_limits(meta, trust, TraceLimits::default())
    }

    fn from_meta_with_limits(
        meta: &Meta,
        trust: TraceTrust,
        limits: TraceLimits,
    ) -> Result<Option<Self>, TraceParseError> {
        let Some(traceparent) = parse_meta_traceparent(meta, limits)? else {
            return Ok(None);
        };
        let tracestate =
            bounded_optional_meta_string(meta, TRACESTATE_KEY, limits.max_tracestate_len)?;
        validate_tracestate(tracestate.as_deref())?;
        let baggage = bounded_optional_meta_string(meta, BAGGAGE_KEY, limits.max_baggage_len)?;
        validate_baggage(baggage.as_deref(), limits.max_baggage_members)?;
        Ok(Some(Self {
            traceparent,
            tracestate,
            baggage,
            trust,
        }))
    }

    fn summary(&self) -> TraceSummary {
        TraceSummary::from_context(self)
    }
}

#[cfg(test)]
impl fmt::Debug for TraceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.summary().fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceSummary {
    trace_id_prefix: Option<String>,
    span_id_prefix: Option<String>,
    sampled: Option<bool>,
    trust: TraceTrust,
    has_tracestate: bool,
    baggage_member_count: usize,
    sensitive_baggage_member_count: usize,
    invalid_reasons: Vec<String>,
}

impl TraceSummary {
    pub fn absent() -> Self {
        Self::absent_with_trust(TraceTrust::Untrusted)
    }

    pub(crate) fn absent_with_trust(trust: TraceTrust) -> Self {
        Self {
            trace_id_prefix: None,
            span_id_prefix: None,
            sampled: None,
            trust,
            has_tracestate: false,
            baggage_member_count: 0,
            sensitive_baggage_member_count: 0,
            invalid_reasons: Vec::new(),
        }
    }

    pub fn invalid(error: &TraceParseError) -> Self {
        let mut summary = Self::absent();
        summary.record_invalid(error);
        summary
    }

    pub fn from_meta(meta: &Meta, trust: TraceTrust) -> Self {
        Self::from_meta_with_limits(meta, trust, TraceLimits::default())
    }

    pub fn from_meta_with_limits(meta: &Meta, trust: TraceTrust, limits: TraceLimits) -> Self {
        let (mut summary, has_valid_traceparent) = match parse_meta_traceparent(meta, limits) {
            Ok(Some(traceparent)) => (Self::from_traceparent(&traceparent, trust), true),
            Ok(None) => (Self::absent_with_trust(trust), false),
            Err(error) => {
                let mut summary = Self::absent_with_trust(trust);
                summary.record_invalid(&error);
                (summary, false)
            }
        };
        match bounded_optional_meta_str(meta, TRACESTATE_KEY, limits.max_tracestate_len) {
            Ok(Some(tracestate)) if has_valid_traceparent => {
                match validate_tracestate(Some(tracestate)) {
                    Ok(()) => summary.has_tracestate = true,
                    Err(error) => summary.record_invalid(&error),
                }
            }
            Ok(Some(_)) => {
                summary.record_invalid(&TraceParseError::TraceStateRequiresTraceParent);
            }
            Ok(None) => {}
            Err(error) => summary.record_invalid(&error),
        };
        match bounded_optional_meta_str(meta, BAGGAGE_KEY, limits.max_baggage_len) {
            Ok(baggage) => match validate_baggage(baggage, limits.max_baggage_members) {
                Ok(()) => {
                    let (baggage_member_count, sensitive_baggage_member_count) =
                        summarize_baggage(baggage);
                    summary.baggage_member_count = baggage_member_count;
                    summary.sensitive_baggage_member_count = sensitive_baggage_member_count;
                }
                Err(error) => summary.record_invalid(&error),
            },
            Err(error) => {
                summary.record_invalid(&error);
            }
        };
        summary
    }

    #[cfg(test)]
    fn from_context(context: &TraceContext) -> Self {
        let (baggage_member_count, sensitive_baggage_member_count) =
            summarize_baggage(context.baggage.as_deref());
        let mut summary = Self::from_traceparent(&context.traceparent, context.trust);
        summary.has_tracestate = context.tracestate.is_some();
        summary.baggage_member_count = baggage_member_count;
        summary.sensitive_baggage_member_count = sensitive_baggage_member_count;
        summary
    }

    fn from_traceparent(traceparent: &TraceParent, trust: TraceTrust) -> Self {
        Self {
            trace_id_prefix: Some(traceparent.trace_id_short().to_owned()),
            span_id_prefix: Some(traceparent.span_id_short().to_owned()),
            sampled: Some(traceparent.sampled()),
            trust,
            has_tracestate: false,
            baggage_member_count: 0,
            sensitive_baggage_member_count: 0,
            invalid_reasons: Vec::new(),
        }
    }

    #[cfg(feature = "http")]
    pub(crate) fn from_valid_traceparent(traceparent: &TraceParent, trust: TraceTrust) -> Self {
        Self::from_traceparent(traceparent, trust)
    }

    fn record_invalid(&mut self, error: &TraceParseError) {
        self.invalid_reasons.push(error.safe_reason());
    }

    #[cfg(feature = "http")]
    pub(crate) fn record_invalid_reason(&mut self, error: &TraceParseError) {
        self.record_invalid(error);
    }

    #[cfg(feature = "http")]
    pub(crate) fn set_has_tracestate_for_http(&mut self) {
        self.has_tracestate = true;
    }

    #[cfg(feature = "http")]
    pub(crate) fn set_baggage_counts_for_http(&mut self, baggage: Option<&str>) {
        let (baggage_member_count, sensitive_baggage_member_count) = summarize_baggage(baggage);
        self.baggage_member_count = baggage_member_count;
        self.sensitive_baggage_member_count = sensitive_baggage_member_count;
    }

    pub fn trace_id_prefix(&self) -> Option<&str> {
        self.trace_id_prefix.as_deref()
    }

    pub fn span_id_prefix(&self) -> Option<&str> {
        self.span_id_prefix.as_deref()
    }

    pub fn sampled(&self) -> Option<bool> {
        self.sampled
    }

    pub fn trust(&self) -> TraceTrust {
        self.trust
    }

    pub fn has_tracestate(&self) -> bool {
        self.has_tracestate
    }

    pub fn baggage_member_count(&self) -> usize {
        self.baggage_member_count
    }

    pub fn sensitive_baggage_member_count(&self) -> usize {
        self.sensitive_baggage_member_count
    }

    pub fn invalid_reasons(&self) -> &[String] {
        &self.invalid_reasons
    }

    pub fn invalid_count(&self) -> usize {
        self.invalid_reasons.len()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceParseError {
    NonStringMeta {
        field: &'static str,
    },
    ValueTooLong {
        field: &'static str,
        actual: usize,
        max: usize,
    },
    TooManyBaggageMembers {
        actual: usize,
        max: usize,
    },
    TooManyTraceStateMembers {
        actual: usize,
        max: usize,
    },
    InvalidTraceParentLength {
        actual: usize,
    },
    InvalidTraceParentFormat,
    UnsupportedVersion,
    InvalidTraceId,
    InvalidSpanId,
    TraceStateRequiresTraceParent,
    InvalidTraceState,
    DuplicateTraceStateKey,
    InvalidBaggageMember,
    #[cfg(feature = "http")]
    MultipleHeaderValues {
        field: &'static str,
    },
    #[cfg(feature = "http")]
    InvalidHeaderValue {
        field: &'static str,
    },
}

impl TraceParseError {
    pub fn safe_reason(&self) -> String {
        match self {
            Self::NonStringMeta { field } => format!("{field} was not a string"),
            Self::ValueTooLong { field, actual, max } => {
                format!("{field} exceeded {max} bytes (actual {actual})")
            }
            Self::TooManyBaggageMembers { actual, max } => {
                format!("baggage exceeded {max} members (actual at least {actual})")
            }
            Self::TooManyTraceStateMembers { actual, max } => {
                format!("tracestate exceeded {max} members (actual at least {actual})")
            }
            Self::InvalidTraceParentLength { actual } => {
                format!("traceparent length was {actual}, expected 55")
            }
            Self::InvalidTraceParentFormat => "traceparent format was invalid".to_owned(),
            Self::UnsupportedVersion => "traceparent version was unsupported".to_owned(),
            Self::InvalidTraceId => "traceparent trace id was invalid".to_owned(),
            Self::InvalidSpanId => "traceparent span id was invalid".to_owned(),
            Self::TraceStateRequiresTraceParent => {
                "tracestate requires a valid traceparent".to_owned()
            }
            Self::InvalidTraceState => "tracestate format was invalid".to_owned(),
            Self::DuplicateTraceStateKey => "tracestate contained a duplicate key".to_owned(),
            Self::InvalidBaggageMember => "baggage member format was invalid".to_owned(),
            #[cfg(feature = "http")]
            Self::MultipleHeaderValues { field } => {
                format!("{field} had multiple header values")
            }
            #[cfg(feature = "http")]
            Self::InvalidHeaderValue { field } => {
                format!("{field} header value was not visible ASCII")
            }
        }
    }
}

impl fmt::Display for TraceParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.safe_reason())
    }
}

impl Error for TraceParseError {}

fn optional_meta_str<'a>(
    meta: &'a Meta,
    field: &'static str,
) -> Result<Option<&'a str>, TraceParseError> {
    match meta.get(field) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.as_str())),
        Some(_) => Err(TraceParseError::NonStringMeta { field }),
    }
}

#[cfg(test)]
fn bounded_optional_meta_string(
    meta: &Meta,
    field: &'static str,
    max: usize,
) -> Result<Option<String>, TraceParseError> {
    bounded_optional_meta_str(meta, field, max).map(|value| value.map(str::to_owned))
}

fn bounded_optional_meta_str<'a>(
    meta: &'a Meta,
    field: &'static str,
    max: usize,
) -> Result<Option<&'a str>, TraceParseError> {
    let Some(value) = optional_meta_str(meta, field)? else {
        return Ok(None);
    };
    if value.len() > max {
        return Err(TraceParseError::ValueTooLong {
            field,
            actual: value.len(),
            max,
        });
    }
    Ok(Some(value))
}

fn parse_meta_traceparent(
    meta: &Meta,
    limits: TraceLimits,
) -> Result<Option<TraceParent>, TraceParseError> {
    let Some(traceparent) = optional_meta_str(meta, TRACEPARENT_KEY)? else {
        return Ok(None);
    };
    if traceparent.len() > limits.max_traceparent_len {
        return Err(TraceParseError::ValueTooLong {
            field: TRACEPARENT_KEY,
            actual: traceparent.len(),
            max: limits.max_traceparent_len,
        });
    }
    parse_traceparent(traceparent).map(Some)
}

#[cfg(feature = "http")]
pub(crate) fn parse_traceparent_value(
    value: &str,
    limits: TraceLimits,
) -> Result<TraceParent, TraceParseError> {
    if value.len() > limits.max_traceparent_len {
        return Err(TraceParseError::ValueTooLong {
            field: TRACEPARENT_KEY,
            actual: value.len(),
            max: limits.max_traceparent_len,
        });
    }
    parse_traceparent(value)
}

fn parse_traceparent(value: &str) -> Result<TraceParent, TraceParseError> {
    if value.len() < TRACEPARENT_V00_LEN {
        return Err(TraceParseError::InvalidTraceParentLength {
            actual: value.len(),
        });
    }
    if !value.is_ascii() {
        return Err(TraceParseError::InvalidTraceParentFormat);
    }
    let bytes = value.as_bytes();
    if bytes[TRACEPARENT_VERSION_END] != b'-'
        || bytes[TRACEPARENT_TRACE_ID_END] != b'-'
        || bytes[TRACEPARENT_SPAN_ID_END] != b'-'
    {
        return Err(TraceParseError::InvalidTraceParentFormat);
    }
    let version = &value[..TRACEPARENT_VERSION_END];
    let trace_id = &value[TRACEPARENT_TRACE_ID_START..TRACEPARENT_TRACE_ID_END];
    let span_id = &value[TRACEPARENT_SPAN_ID_START..TRACEPARENT_SPAN_ID_END];
    let flags = &value[TRACEPARENT_FLAGS_START..TRACEPARENT_FLAGS_END];
    if !is_lower_hex(version)
        || !is_lower_hex(trace_id)
        || !is_lower_hex(span_id)
        || !is_lower_hex(flags)
    {
        return Err(TraceParseError::InvalidTraceParentFormat);
    }
    if version == "ff" {
        return Err(TraceParseError::UnsupportedVersion);
    }
    if version == "00" && value.len() != TRACEPARENT_V00_LEN {
        return Err(TraceParseError::InvalidTraceParentLength {
            actual: value.len(),
        });
    }
    if version != "00"
        && value.len() > TRACEPARENT_V00_LEN
        && bytes[TRACEPARENT_NEXT_SEPARATOR] != b'-'
    {
        return Err(TraceParseError::InvalidTraceParentFormat);
    }
    if trace_id.bytes().all(|b| b == b'0') {
        return Err(TraceParseError::InvalidTraceId);
    }
    if span_id.bytes().all(|b| b == b'0') {
        return Err(TraceParseError::InvalidSpanId);
    }
    let flag_byte =
        u8::from_str_radix(flags, 16).map_err(|_| TraceParseError::InvalidTraceParentFormat)?;
    Ok(TraceParent {
        trace_id: trace_id.to_owned(),
        span_id: span_id.to_owned(),
        sampled: flag_byte & 0x01 == 0x01,
    })
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn summarize_baggage(baggage: Option<&str>) -> (usize, usize) {
    let Some(baggage) = baggage else {
        return (0, 0);
    };
    let mut total = 0;
    let mut sensitive = 0;
    for key in baggage_keys(baggage) {
        total += 1;
        if is_sensitive_key(key) {
            sensitive += 1;
        }
    }
    (total, sensitive)
}

fn validate_tracestate(tracestate: Option<&str>) -> Result<(), TraceParseError> {
    let Some(tracestate) = tracestate else {
        return Ok(());
    };
    let mut keys = BTreeSet::new();
    let mut total = 0;
    for member in tracestate.split(',') {
        let member = member.trim();
        if member.is_empty() {
            return Err(TraceParseError::InvalidTraceState);
        }
        total += 1;
        if total > MAX_TRACESTATE_MEMBERS {
            return Err(TraceParseError::TooManyTraceStateMembers {
                actual: total,
                max: MAX_TRACESTATE_MEMBERS,
            });
        }
        let Some((key, value)) = member.split_once('=') else {
            return Err(TraceParseError::InvalidTraceState);
        };
        let key = key.trim();
        if !is_valid_tracestate_key(key) || !is_valid_tracestate_value(value) {
            return Err(TraceParseError::InvalidTraceState);
        }
        if !keys.insert(key) {
            return Err(TraceParseError::DuplicateTraceStateKey);
        }
    }
    Ok(())
}

fn validate_baggage(baggage: Option<&str>, max: usize) -> Result<(), TraceParseError> {
    let Some(baggage) = baggage else {
        return Ok(());
    };
    let mut total = 0;
    for member in baggage
        .split(',')
        .map(str::trim)
        .filter(|member| !member.is_empty())
    {
        total += 1;
        if total > max {
            return Err(TraceParseError::TooManyBaggageMembers { actual: total, max });
        }
        let Some((key, rest)) = member.split_once('=') else {
            return Err(TraceParseError::InvalidBaggageMember);
        };
        if !is_valid_baggage_key(key.trim()) {
            return Err(TraceParseError::InvalidBaggageMember);
        }
        let mut value_and_props = rest.split(';');
        let value = value_and_props.next().unwrap_or("").trim();
        if !is_valid_baggage_value(value) {
            return Err(TraceParseError::InvalidBaggageMember);
        }
        for property in value_and_props {
            let property = property.trim();
            if property.is_empty() {
                return Err(TraceParseError::InvalidBaggageMember);
            }
            if let Some((property_key, property_value)) = property.split_once('=') {
                if !is_valid_baggage_key(property_key.trim())
                    || !is_valid_baggage_value(property_value.trim())
                {
                    return Err(TraceParseError::InvalidBaggageMember);
                }
            } else if !is_valid_baggage_key(property) {
                return Err(TraceParseError::InvalidBaggageMember);
            }
        }
    }
    Ok(())
}

#[cfg(feature = "http")]
pub(crate) fn validate_tracestate_value(tracestate: Option<&str>) -> Result<(), TraceParseError> {
    validate_tracestate(tracestate)
}

#[cfg(feature = "http")]
pub(crate) fn validate_baggage_value(
    baggage: Option<&str>,
    max_members: usize,
) -> Result<(), TraceParseError> {
    validate_baggage(baggage, max_members)
}

fn baggage_keys(baggage: &str) -> impl Iterator<Item = &str> {
    baggage.split(',').filter_map(|member| {
        let (key, _) = member.split_once('=')?;
        let key = key.trim();
        (!key.is_empty()).then_some(key)
    })
}

fn is_sensitive_key(key: &str) -> bool {
    const SENSITIVE_KEYS: &[&str] = &[
        "authorization",
        "cookie",
        "setcookie",
        "password",
        "secret",
        "token",
        "accesstoken",
        "refreshtoken",
        "apikey",
        "xapikey",
        "privatekey",
        "session",
        "sessionid",
    ];
    SENSITIVE_KEYS
        .iter()
        .any(|sensitive_key| normalized_ascii_key_eq(key, sensitive_key))
}

fn normalized_ascii_key_eq(key: &str, expected: &str) -> bool {
    let mut key_bytes = key
        .bytes()
        .filter(|byte| byte.is_ascii_alphanumeric())
        .map(|byte| byte.to_ascii_lowercase());
    let mut expected_bytes = expected.bytes();
    loop {
        match (key_bytes.next(), expected_bytes.next()) {
            (Some(left), Some(right)) if left == right => {}
            (None, None) => return true,
            _ => return false,
        }
    }
}

fn is_valid_tracestate_key(key: &str) -> bool {
    !key.is_empty()
        && key.is_ascii()
        && key.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'_' | b'-' | b'*' | b'/' | b'@')
        })
}

fn is_valid_tracestate_value(value: &str) -> bool {
    !value.is_empty()
        && value.is_ascii()
        && value.trim() == value
        && value
            .bytes()
            .all(|byte| matches!(byte, 0x20..=0x2b | 0x2d..=0x3c | 0x3e..=0x7e))
}

fn is_valid_baggage_key(key: &str) -> bool {
    !key.is_empty()
        && key.is_ascii()
        && key.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

fn is_valid_baggage_value(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| matches!(byte, 0x21 | 0x23..=0x2b | 0x2d..=0x3a | 0x3c..=0x5b | 0x5d..=0x7e))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_TRACEPARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01";

    #[test]
    fn trace_context_parses_meta_and_summarizes_safely() {
        let mut meta = Meta::new();
        meta.set_traceparent(VALID_TRACEPARENT);
        meta.set_tracestate("vendor=value");
        meta.set_baggage("region=us-east-1,accessToken=super-secret-token");

        let context = TraceContext::from_meta(&meta, TraceTrust::Untrusted)
            .expect("valid trace metadata")
            .expect("trace context exists");

        let summary = context.summary();

        assert_eq!(summary.trace_id_prefix(), Some("0af76519"));
        assert_eq!(summary.span_id_prefix(), Some("00f067aa"));
        assert_eq!(summary.sampled(), Some(true));
        assert_eq!(summary.trust(), TraceTrust::Untrusted);
        assert!(summary.has_tracestate());
        assert_eq!(summary.baggage_member_count(), 2);
        assert_eq!(summary.sensitive_baggage_member_count(), 1);
        assert_eq!(summary.invalid_count(), 0);
    }

    #[test]
    fn malformed_traceparents_are_rejected() {
        for value in [
            "",
            "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7",
            "ff-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01",
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
            "00-0af7651916cd43dd8448eb211c80319c-0000000000000000-01",
            "00-0AF7651916CD43DD8448EB211C80319C-00f067aa0ba902b7-01",
            "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-zz",
        ] {
            assert!(
                TraceParent::parse(value).is_err(),
                "{value} should be rejected"
            );
        }
    }

    #[test]
    fn non_ascii_traceparents_are_rejected_without_panicking() {
        let value = format!("{}\u{00e9}", &VALID_TRACEPARENT[..54]);
        let result = std::panic::catch_unwind(|| TraceParent::parse(&value));

        assert!(result.is_ok(), "non-ASCII input must not panic");
        assert!(matches!(
            result.unwrap(),
            Err(TraceParseError::InvalidTraceParentFormat)
        ));

        let higher_version = "01-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01-\u{00e9}";
        assert!(matches!(
            TraceParent::parse(higher_version),
            Err(TraceParseError::InvalidTraceParentFormat)
        ));
    }

    #[test]
    fn traceparent_version_rules_cover_v00_and_higher_version_bounds() {
        assert!(matches!(
            TraceParent::parse(&format!("{VALID_TRACEPARENT}-extra")),
            Err(TraceParseError::InvalidTraceParentLength { actual }) if actual == 61
        ));

        let higher_base = "01-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01";
        let max_extra_len = 512 - higher_base.len() - 1;
        let max_len_value = format!("{higher_base}-{}", "a".repeat(max_extra_len));
        assert_eq!(max_len_value.len(), 512);
        TraceParent::parse(&max_len_value).expect("512-byte higher version should be accepted");

        let too_long = format!("{higher_base}-{}", "a".repeat(max_extra_len + 1));
        assert_eq!(too_long.len(), 513);
        assert!(matches!(
            TraceParent::parse(&too_long),
            Err(TraceParseError::ValueTooLong {
                field: TRACEPARENT_KEY,
                actual: 513,
                max: 512,
            })
        ));
    }

    #[test]
    fn higher_version_traceparents_preserve_stable_fields() {
        let traceparent =
            TraceParent::parse("01-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01-extra")
                .expect("higher versions can carry additive fields");

        assert_eq!(traceparent.trace_id(), "0af7651916cd43dd8448eb211c80319c");
        assert_eq!(traceparent.span_id(), "00f067aa0ba902b7");
        assert!(traceparent.sampled());
    }

    #[test]
    fn strict_context_rejects_oversized_optional_metadata() {
        let mut meta = Meta::new();
        meta.set_traceparent("x".repeat(4096));
        assert!(TraceContext::from_meta(&meta, TraceTrust::Untrusted).is_err());

        let mut meta = Meta::new();
        meta.set_traceparent(VALID_TRACEPARENT);
        meta.set_tracestate("v".repeat(20));
        let limits = TraceLimits {
            max_tracestate_len: 8,
            ..TraceLimits::default()
        };
        assert!(TraceContext::from_meta_with_limits(&meta, TraceTrust::Untrusted, limits).is_err());

        let mut meta = Meta::new();
        meta.set_traceparent(VALID_TRACEPARENT);
        meta.set_baggage("a".repeat(20));
        let limits = TraceLimits {
            max_baggage_len: 8,
            ..TraceLimits::default()
        };
        assert!(TraceContext::from_meta_with_limits(&meta, TraceTrust::Untrusted, limits).is_err());
    }

    #[test]
    fn strict_context_rejects_invalid_optional_metadata() {
        let mut meta = Meta::new();
        meta.set_traceparent(VALID_TRACEPARENT);
        meta.set_tracestate("vendor=value,vendor=other");
        let error = TraceContext::from_meta(&meta, TraceTrust::Untrusted)
            .expect_err("duplicate tracestate keys should be rejected");
        assert!(matches!(error, TraceParseError::DuplicateTraceStateKey));

        let mut meta = Meta::new();
        meta.set_traceparent(VALID_TRACEPARENT);
        meta.set_baggage("a=1,b=2,c=3");
        let limits = TraceLimits {
            max_baggage_members: 2,
            ..TraceLimits::default()
        };
        let error = TraceContext::from_meta_with_limits(&meta, TraceTrust::Untrusted, limits)
            .expect_err("baggage member cap should be enforced");

        assert!(matches!(
            error,
            TraceParseError::TooManyBaggageMembers { actual: 3, max: 2 }
        ));
        assert!(!error.safe_reason().contains("a=1"));
    }
}
