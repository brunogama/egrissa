use egress_core::ids::FeedbackKey;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tokio::sync::Semaphore;
use transport_core::{
    CloseFuture, EstablishmentFuture, EstablishmentRequest, Transport, TransportFailure,
    TransportOutcome,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptedStep {
    Succeed,
    Fail(TransportFailure),
    AwaitRelease(Box<ScriptedStep>),
    SucceedThenFail(TransportFailure),
}

#[derive(Default)]
struct State {
    script: VecDeque<ScriptedStep>,
    establishments: Vec<FeedbackKey>,
    waiting: usize,
    session_failures: Vec<(FeedbackKey, TransportFailure)>,
}

struct Inner {
    state: Mutex<State>,
    gate: Semaphore,
}

#[derive(Clone)]
pub struct FakeTransport {
    inner: Arc<Inner>,
}

impl FakeTransport {
    pub fn new(script: impl IntoIterator<Item = ScriptedStep>) -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State {
                    script: script.into_iter().collect(),
                    ..State::default()
                }),
                gate: Semaphore::new(0),
            }),
        }
    }

    pub fn release(&self, permits: usize) {
        self.inner.gate.add_permits(permits);
    }

    pub fn establishments(&self) -> Vec<FeedbackKey> {
        self.inner
            .state
            .lock()
            .expect("fake transport mutex poisoned")
            .establishments
            .clone()
    }

    pub fn waiting(&self) -> usize {
        self.inner
            .state
            .lock()
            .expect("fake transport mutex poisoned")
            .waiting
    }

    pub fn pending_session_failures(&self) -> Vec<(FeedbackKey, TransportFailure)> {
        std::mem::take(
            &mut self
                .inner
                .state
                .lock()
                .expect("fake transport mutex poisoned")
                .session_failures,
        )
    }
}

impl Transport for FakeTransport {
    fn establish(&self, request: EstablishmentRequest) -> EstablishmentFuture {
        let fake = self.clone();
        Box::pin(async move {
            let key = request.plan.key();
            let mut step = {
                let mut state = fake
                    .inner
                    .state
                    .lock()
                    .expect("fake transport mutex poisoned");
                state.establishments.push(key);
                state.script.pop_front().unwrap_or(ScriptedStep::Succeed)
            };
            if let ScriptedStep::AwaitRelease(next) = step {
                {
                    let mut state = fake
                        .inner
                        .state
                        .lock()
                        .expect("fake transport mutex poisoned");
                    state.waiting += 1;
                }
                let permit = fake.inner.gate.acquire().await.expect("fake gate closed");
                permit.forget();
                {
                    let mut state = fake
                        .inner
                        .state
                        .lock()
                        .expect("fake transport mutex poisoned");
                    state.waiting -= 1;
                }
                step = *next;
            }
            match step {
                ScriptedStep::Succeed => TransportOutcome::Established,
                ScriptedStep::Fail(failure) => TransportOutcome::Failed(failure),
                ScriptedStep::SucceedThenFail(failure) => {
                    fake.inner
                        .state
                        .lock()
                        .expect("fake transport mutex poisoned")
                        .session_failures
                        .push((key, failure));
                    TransportOutcome::Established
                }
                ScriptedStep::AwaitRelease(_) => unreachable!(),
            }
        })
    }

    fn close(&self, _key: FeedbackKey) -> CloseFuture {
        Box::pin(async {})
    }
}
