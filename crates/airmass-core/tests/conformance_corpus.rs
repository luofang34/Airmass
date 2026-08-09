//! Executable tests for the source-neutral weather ingress corpus.

#![allow(clippy::expect_used, clippy::panic)]

use airmass_core::{
    AbsenceEvidence, AbsenceReason, AbsentField, AcceptStatus,
    CURRENT_WEATHER_INGRESS_SCHEMA_VERSION, Cancellation, CancellationReason, DeadlineSet,
    DeliveryPath, DurationMillis, EvaluationTime, FieldState, MonotonicTime, ProducerInstanceId,
    ProductProvenance, ProductRevision, QualityEvidence, SelectionCriterion, SnapshotRevision,
    SourceEpoch, SourceId, SourceProductRevision, SourceRef, SourceTime, SourceTimeKind,
    StoreConfig, TimeBasis, TimeEvidence, TimeQuality, TimeUncertainty, TimedField, UtcInterval,
    UtcTime, WeatherIngress, WeatherOrigin, WeatherPayload, WeatherProduct, WeatherProductId,
    WeatherProductState, WeatherSnapshotHandle, WeatherStore,
};
use serde::Deserialize;

const PRODUCT_ID: &str = "corpus-weather-product";

#[derive(Debug, Deserialize)]
struct Corpus {
    corpus_version: u16,
    weather_ingress_schema_version: u16,
    evidence_profile: EvidenceProfile,
    scenarios: Vec<Scenario>,
}

#[derive(Debug, Deserialize)]
struct EvidenceProfile {
    origin: String,
    delivery_path: String,
    quality: String,
    known_time_basis: String,
    known_time_quality: String,
    known_time_uncertainty: u64,
    raw_product_identity: Option<RawIdentity>,
}

#[derive(Debug, Deserialize)]
struct RawIdentity {
    namespace: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct Scenario {
    name: String,
    steps: Vec<Step>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Product,
    Cancellation,
    Advance,
}

#[derive(Debug, Deserialize)]
struct Step {
    operation: Operation,
    utc: i64,
    monotonic: u64,
    source: Option<String>,
    epoch: Option<u64>,
    source_revision: Option<u64>,
    source_time: Option<i64>,
    payload: Option<Vec<u8>>,
    valid_end: Option<i64>,
    maximum_age: Option<u64>,
    expect: Expected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ExpectedStatus {
    Applied,
    Replay,
    Stale,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ExpectedState {
    Available,
    Cancelled,
    Expired,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ExpectedCriterion {
    SoleSource,
    KnownSourceTime,
}

#[derive(Debug, Deserialize)]
struct Expected {
    status: Option<ExpectedStatus>,
    published: bool,
    state: ExpectedState,
    product_revision: u64,
    snapshot_revision: u64,
    selected_source: String,
    selected_epoch: u64,
    criterion: ExpectedCriterion,
    next_utc: Option<i64>,
    next_monotonic: Option<u64>,
}

struct StepResult {
    status: Option<AcceptStatus>,
    published: bool,
    deadline: DeadlineSet,
    handle: WeatherSnapshotHandle,
}

#[test]
fn source_neutral_corpus_matches_the_public_core_contract() {
    let corpus: Corpus =
        serde_json::from_str(include_str!("../../../conformance/weather-ingress-v3.json"))
            .expect("conformance corpus must parse");
    assert_eq!(corpus.corpus_version, 1);
    assert_eq!(
        corpus.weather_ingress_schema_version,
        CURRENT_WEATHER_INGRESS_SCHEMA_VERSION
    );
    assert_evidence_profile(&corpus.evidence_profile);
    for (index, scenario) in corpus.scenarios.iter().enumerate() {
        run_scenario(scenario, index as u64 + 1);
    }
}

fn assert_evidence_profile(profile: &EvidenceProfile) {
    assert_eq!(profile.origin, "simulation");
    assert_eq!(profile.delivery_path, "conformance-corpus");
    assert_eq!(profile.quality, "unavailable");
    assert_eq!(profile.known_time_basis, "source_utc");
    assert_eq!(profile.known_time_quality, "exact");
    assert_eq!(profile.known_time_uncertainty, 0);
    if let Some(identity) = &profile.raw_product_identity {
        assert!(!identity.namespace.is_empty());
        assert!(!identity.value.is_empty());
    }
    assert!(profile.raw_product_identity.is_none());
}

fn run_scenario(scenario: &Scenario, producer_id: u64) {
    let mut store = WeatherStore::new(StoreConfig::default(), ProducerInstanceId::new(producer_id))
        .expect("corpus configuration must be valid");
    for step in &scenario.steps {
        let result = apply_step(&mut store, step);
        assert_step(&scenario.name, step, &result);
    }
}

fn apply_step(store: &mut WeatherStore, step: &Step) -> StepResult {
    let now = EvaluationTime::new(
        UtcTime::from_unix_millis(step.utc),
        monotonic_millis(step.monotonic),
    );
    let (status, published, deadline) = match step.operation {
        Operation::Advance => {
            let outcome = store.advance_time(now).expect("time advance must succeed");
            (
                None,
                outcome.publication().is_some(),
                outcome.next_deadline(),
            )
        }
        Operation::Product | Operation::Cancellation => {
            let ingress = make_ingress(step);
            let outcome = store.accept(ingress, now).expect("ingress must succeed");
            (
                Some(outcome.status()),
                outcome.publication().is_some(),
                outcome.next_deadline(),
            )
        }
    };
    StepResult {
        status,
        published,
        deadline,
        handle: store.capture(),
    }
}

fn make_ingress(step: &Step) -> WeatherIngress {
    let source = step
        .source
        .as_deref()
        .expect("source step must name a source");
    let epoch = step.epoch.expect("source step must name an epoch");
    let revision = step
        .source_revision
        .expect("source step must name a revision");
    match step.operation {
        Operation::Product => WeatherIngress::product(
            WeatherProductId::new(PRODUCT_ID),
            SourceProductRevision::new(revision),
            make_product(step, source, epoch),
        ),
        Operation::Cancellation => WeatherIngress::cancellation(
            WeatherProductId::new(PRODUCT_ID),
            SourceProductRevision::new(revision),
            Cancellation::new(
                timed(CancellationReason::Withdrawn, step, source, epoch),
                absent(),
            ),
        ),
        Operation::Advance => panic!("an advance step has no ingress"),
    }
}

fn make_product(step: &Step, source: &str, epoch: u64) -> WeatherProduct {
    let payload = step
        .payload
        .clone()
        .expect("product step must have payload bytes");
    let validity = step.valid_end.map_or_else(absent, |end| {
        FieldState::present(timed(
            UtcInterval::new(UtcTime::from_unix_millis(0), UtcTime::from_unix_millis(end)),
            step,
            source,
            epoch,
        ))
    });
    let maximum_age = step.maximum_age.map_or_else(absent, |millis| {
        FieldState::present(timed(DurationMillis::new(millis), step, source, epoch))
    });
    WeatherProduct::new(
        timed(
            WeatherPayload::new("application/octet-stream", payload),
            step,
            source,
            epoch,
        ),
        absent(),
        absent(),
        absent(),
        validity,
        maximum_age,
    )
}

fn timed<T>(value: T, step: &Step, source: &str, epoch: u64) -> TimedField<T> {
    TimedField::new(
        value,
        TimeEvidence::new(
            monotonic_millis(step.monotonic),
            source_time(step.source_time),
        ),
        QualityEvidence::Unavailable,
        ProductProvenance::new(
            SourceRef::new(SourceId::new(source), SourceEpoch::new(epoch)),
            WeatherOrigin::Simulation,
            DeliveryPath::new("conformance-corpus"),
            None,
        ),
    )
}

fn source_time(value: Option<i64>) -> SourceTime {
    value.map_or(SourceTime::Unknown, |millis| SourceTime::Known {
        value: UtcTime::from_unix_millis(millis),
        kind: SourceTimeKind::Product,
        basis: TimeBasis::SourceUtc,
        quality: TimeQuality::Exact,
        uncertainty: TimeUncertainty::Bounded(0),
    })
}

fn absent<T>() -> FieldState<T> {
    FieldState::absent(AbsentField::new(
        AbsenceReason::NotObserved,
        AbsenceEvidence::Unknown,
    ))
}

fn assert_step(scenario: &str, step: &Step, result: &StepResult) {
    assert_eq!(
        result.status.map(expected_status),
        step.expect.status,
        "status in {scenario}"
    );
    assert_eq!(
        result.published, step.expect.published,
        "publication in {scenario}"
    );
    assert_eq!(
        result.handle.snapshot_revision(),
        SnapshotRevision::new(step.expect.snapshot_revision),
        "snapshot revision in {scenario}"
    );
    assert_deadline(scenario, step, result.deadline);
    assert_product(scenario, step, &result.handle);
}

fn assert_deadline(scenario: &str, step: &Step, deadline: DeadlineSet) {
    assert_eq!(
        deadline.utc.map(UtcTime::as_unix_millis),
        step.expect.next_utc,
        "UTC deadline in {scenario}"
    );
    assert_eq!(
        deadline.monotonic.map(|value| value.as_micros() / 1_000),
        step.expect.next_monotonic,
        "monotonic deadline in {scenario}"
    );
}

fn monotonic_millis(value: u64) -> MonotonicTime {
    MonotonicTime::from_micros(value * 1_000)
}

fn assert_product(scenario: &str, step: &Step, handle: &WeatherSnapshotHandle) {
    let product = handle
        .snapshot()
        .product(&WeatherProductId::new(PRODUCT_ID))
        .expect("corpus product must exist");
    assert_eq!(
        product.product_revision(),
        ProductRevision::new(step.expect.product_revision),
        "product revision in {scenario}"
    );
    assert_eq!(
        state(product.state()),
        step.expect.state,
        "state in {scenario}"
    );
    assert_eq!(
        product.selection().selected_source.id.as_str(),
        step.expect.selected_source,
        "selected source in {scenario}"
    );
    assert_eq!(
        product.selection().selected_source.epoch,
        SourceEpoch::new(step.expect.selected_epoch),
        "selected epoch in {scenario}"
    );
    assert_eq!(
        criterion(product.selection().decisive_criterion),
        step.expect.criterion,
        "selection criterion in {scenario}"
    );
}

fn expected_status(status: AcceptStatus) -> ExpectedStatus {
    match status {
        AcceptStatus::Applied => ExpectedStatus::Applied,
        AcceptStatus::Replay => ExpectedStatus::Replay,
        AcceptStatus::Stale => ExpectedStatus::Stale,
        _ => panic!("corpus does not know this status"),
    }
}

fn state(state: &WeatherProductState) -> ExpectedState {
    match state {
        WeatherProductState::Available { .. } => ExpectedState::Available,
        WeatherProductState::Cancelled { .. } => ExpectedState::Cancelled,
        WeatherProductState::Absent { absence } if absence.reason() == AbsenceReason::Expired => {
            ExpectedState::Expired
        }
        WeatherProductState::Absent { .. } => panic!("corpus does not know this absence"),
    }
}

fn criterion(value: SelectionCriterion) -> ExpectedCriterion {
    match value {
        SelectionCriterion::SoleSource => ExpectedCriterion::SoleSource,
        SelectionCriterion::KnownSourceTime => ExpectedCriterion::KnownSourceTime,
        _ => panic!("corpus does not know this selection criterion"),
    }
}
