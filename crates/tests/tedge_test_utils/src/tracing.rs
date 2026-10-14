//! Captures tracing events in-process for test assertions on log level and message content
use std::sync::Arc;
use std::sync::Mutex;
use tracing::Level;
use tracing::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

#[derive(Clone, Debug)]
pub struct CapturedEvent {
    pub level: Level,
    pub target: String,
    pub message: String,
}

#[derive(Clone, Default)]
pub struct TracingCapture {
    events: Arc<Mutex<Vec<CapturedEvent>>>,
}

impl TracingCapture {
    pub fn events(&self) -> Vec<CapturedEvent> {
        self.events.lock().unwrap().clone()
    }

    pub fn messages(&self) -> Vec<String> {
        self.events().into_iter().map(|e| e.message).collect()
    }

    pub fn at_or_above(&self, max_level: Level) -> Vec<CapturedEvent> {
        self.events()
            .into_iter()
            .filter(|e| e.level <= max_level)
            .collect()
    }

    pub fn at(&self, level: Level) -> Vec<String> {
        self.events()
            .into_iter()
            .filter(|e| e.level == level)
            .map(|e| e.message)
            .collect()
    }

    fn assert_none_at(events: &[CapturedEvent], label: &str) {
        assert!(
            events.is_empty(),
            "Expected no {label} but got:\n{}",
            events
                .iter()
                .map(|e| format!("  {}: {}", e.level, e.message))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    pub fn assert_no_errors(&self) {
        Self::assert_none_at(&self.at_or_above(Level::ERROR), "errors");
    }

    pub fn assert_no_errors_or_warnings(&self) {
        Self::assert_none_at(&self.at_or_above(Level::WARN), "errors or warnings");
    }

    /// Installs this capture as the thread-local tracing subscriber.
    /// Returns a guard that restores the previous subscriber on drop.
    pub fn start(self) -> (Self, tracing::subscriber::DefaultGuard) {
        let guard = tracing_subscriber::registry()
            .with(CaptureLayer {
                events: self.events.clone(),
            })
            .set_default();
        (self, guard)
    }

    /// Installs this capture as the process-wide subscriber (once per process)
    pub fn start_global(self) -> Self {
        let subscriber = tracing_subscriber::registry().with(CaptureLayer {
            events: self.events.clone(),
        });
        tracing::subscriber::set_global_default(subscriber)
            .expect("set_global_default can only be called once per process");
        self
    }
}

struct CaptureLayer {
    events: Arc<Mutex<Vec<CapturedEvent>>>,
}

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);
        self.events.lock().unwrap().push(CapturedEvent {
            level: *event.metadata().level(),
            target: event.metadata().target().to_string(),
            message: visitor.0,
        });
    }
}

struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}
