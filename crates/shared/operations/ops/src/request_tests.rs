use serde::{Deserialize, Serialize};

use super::*;
use crate::{CapabilitySupport, OperationPlan, RetryClass, Reversibility, RiskClass};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Params {
    host: String,
    container: String,
}

struct Restart;

impl OperationDefinition for Restart {
    type Parameters = Params;
    type Output = serde_json::Value;

    fn spec() -> OperationSpec {
        OperationSpec::new(
            OperationName::new("container.restart").unwrap(),
            TargetKind::Container,
            AccessClass::Mutation,
        )
        .with_safety(RiskClass::Disruptive, Reversibility::Reversible)
        .with_lifecycle(
            CapabilitySupport::Optional,
            CapabilitySupport::Optional,
            CapabilitySupport::Optional,
            CapabilitySupport::Required,
            CapabilitySupport::Unsupported,
        )
        .with_retry(RetryClass::Safe, true)
    }

    fn target(parameters: &Self::Parameters) -> Result<TargetRef, TargetRefError> {
        TargetRef::new(TargetKind::Container, parameters.container.clone())?
            .with_host(parameters.host.clone())
    }
}

fn parameters() -> Params {
    Params {
        host: "devhost".into(),
        container: "soma".into(),
    }
}

fn authorized_request(now: Timestamp) -> OperationRequest<Params> {
    let context = OperationContext::new()
        .with_idempotency_key(IdempotencyKey::new("restart:devhost:soma:1").unwrap())
        .with_deadline(Timestamp::from_unix_millis(now.unix_millis() + 10_000));
    let request = OperationRequest::new::<Restart>(context, parameters()).unwrap();
    let authorization = AuthorizationEvidence::new(
        ProducerRef::new("soma", "0.4.1").unwrap(),
        AuthorizationScope::new(request.operation().clone(), request.target().clone()),
        Timestamp::from_unix_millis(now.unix_millis() - 1),
        Timestamp::from_unix_millis(now.unix_millis() + 1_000),
    )
    .unwrap();
    request.with_authorization(authorization)
}

#[test]
fn mutation_requires_authorization() {
    let now = Timestamp::from_unix_millis(100);
    let context = OperationContext::new()
        .with_idempotency_key(IdempotencyKey::new("restart:devhost:soma:1").unwrap())
        .with_deadline(Timestamp::from_unix_millis(1_000));
    let request = OperationRequest::new::<Restart>(context, parameters()).unwrap();
    assert_eq!(
        request.validate_against(&Restart::spec(), now, None),
        Err(RequestError::MissingAuthorization)
    );
}

#[test]
fn idempotent_mutation_requires_key() {
    let now = Timestamp::from_unix_millis(100);
    let request = OperationRequest::new::<Restart>(OperationContext::new(), parameters()).unwrap();
    assert_eq!(
        request.validate_against(&Restart::spec(), now, None),
        Err(RequestError::MissingIdempotencyKey)
    );
}

#[test]
fn authorization_is_exactly_bound_to_request() {
    let now = Timestamp::from_unix_millis(100);
    authorized_request(now)
        .validate_against(&Restart::spec(), now, None)
        .unwrap();

    let request = authorized_request(now);
    let wrong = OperationSpec::new(
        OperationName::new("container.stop").unwrap(),
        TargetKind::Container,
        AccessClass::Mutation,
    );
    assert_eq!(
        request.validate_against(&wrong, now, None),
        Err(RequestError::OperationMismatch)
    );
}

#[test]
fn expired_authorization_is_rejected() {
    let now = Timestamp::from_unix_millis(100);
    let request = authorized_request(now);
    assert_eq!(
        request.validate_against(&Restart::spec(), Timestamp::from_unix_millis(1_100), None),
        Err(RequestError::Authorization(AuthorizationError::Expired))
    );
}

#[test]
fn plan_binding_must_match_exactly() {
    let now = Timestamp::from_unix_millis(100);
    let context = OperationContext::new()
        .with_idempotency_key(IdempotencyKey::new("restart:devhost:soma:1").unwrap())
        .with_deadline(Timestamp::from_unix_millis(10_000));
    let request = OperationRequest::new::<Restart>(context, parameters()).unwrap();
    let plan = OperationPlan::new(
        request.context().operation_id().clone(),
        request.operation().clone(),
        request.target().clone(),
        RiskClass::Disruptive,
        Reversibility::Reversible,
    )
    .unwrap();
    let authorization = AuthorizationEvidence::new(
        ProducerRef::new("soma", "0.4.1").unwrap(),
        AuthorizationScope::new(request.operation().clone(), request.target().clone()),
        Timestamp::from_unix_millis(99),
        Timestamp::from_unix_millis(1_000),
    )
    .unwrap()
    .with_plan_fingerprint(plan.fingerprint().clone());
    let request = request.with_authorization(authorization);

    request
        .validate_against(&Restart::spec(), now, Some(plan.fingerprint()))
        .unwrap();
    assert_eq!(
        request.validate_against(&Restart::spec(), now, None),
        Err(RequestError::Authorization(
            AuthorizationError::PlanMismatch
        ))
    );
}
