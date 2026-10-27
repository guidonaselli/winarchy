use std::time::{Duration, Instant};

pub const LISTEN_CAP: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Listening,
    Thinking,
    Speaking,
    Error,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Listening => "listening",
            State::Thinking => "thinking",
            State::Speaking => "speaking",
            State::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    PttDown,
    PttUp,
    ReplyReady,
    SpeechDone,
    Failed,
}

/// Qué hacer con el trabajo en curso al cambiar de estado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    StartCapture,
    StopCapture,
    /// Barge-in: cortar voz o respuesta en vuelo y empezar a grabar.
    CancelAndCapture,
}

pub struct Machine {
    state: State,
    since: Instant,
}

impl Machine {
    pub fn new(now: Instant) -> Self {
        Self { state: State::Idle, since: now }
    }

    pub fn state(&self) -> State {
        self.state
    }

    fn go(&mut self, s: State, now: Instant) {
        self.state = s;
        self.since = now;
    }

    pub fn on(&mut self, e: Event, now: Instant) -> Effect {
        use State::*;
        match (self.state, e) {
            (Idle | Error, Event::PttDown) => {
                self.go(Listening, now);
                Effect::StartCapture
            }
            (Thinking | Speaking, Event::PttDown) => {
                self.go(Listening, now);
                Effect::CancelAndCapture
            }
            (Listening, Event::PttUp) => {
                self.go(Thinking, now);
                Effect::StopCapture
            }
            (Thinking, Event::ReplyReady) => {
                self.go(Speaking, now);
                Effect::None
            }
            (Speaking, Event::SpeechDone) => {
                self.go(Idle, now);
                Effect::None
            }
            (Listening | Thinking | Speaking, Event::Failed) => {
                self.go(Error, now);
                Effect::None
            }
            _ => Effect::None,
        }
    }

    /// Un micrófono colgado (se perdió el key-up) se corta solo a los 30 s.
    pub fn tick(&mut self, now: Instant) -> Effect {
        if self.state == State::Listening && now.duration_since(self.since) >= LISTEN_CAP {
            self.go(State::Thinking, now);
            return Effect::StopCapture;
        }
        Effect::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: u64, t0: Instant) -> Instant {
        t0 + Duration::from_secs(s)
    }

    #[test]
    fn happy_path() {
        let t = Instant::now();
        let mut m = Machine::new(t);
        assert_eq!(m.on(Event::PttDown, t), Effect::StartCapture);
        assert_eq!(m.on(Event::PttUp, t), Effect::StopCapture);
        assert_eq!(m.state(), State::Thinking);
        m.on(Event::ReplyReady, t);
        assert_eq!(m.state(), State::Speaking);
        m.on(Event::SpeechDone, t);
        assert_eq!(m.state(), State::Idle);
    }

    #[test]
    fn barge_in_cancels_speech_and_thinking() {
        let t = Instant::now();
        for from in [Event::PttUp, Event::ReplyReady] {
            let mut m = Machine::new(t);
            m.on(Event::PttDown, t);
            m.on(Event::PttUp, t);
            if from == Event::ReplyReady {
                m.on(Event::ReplyReady, t);
            }
            assert_eq!(m.on(Event::PttDown, t), Effect::CancelAndCapture);
            assert_eq!(m.state(), State::Listening);
        }
    }

    #[test]
    fn stuck_mic_stops_at_cap() {
        let t = Instant::now();
        let mut m = Machine::new(t);
        m.on(Event::PttDown, t);
        assert_eq!(m.tick(at(29, t)), Effect::None);
        assert_eq!(m.tick(at(30, t)), Effect::StopCapture);
        assert_eq!(m.state(), State::Thinking);
    }

    #[test]
    fn late_key_up_after_cap_is_ignored() {
        let t = Instant::now();
        let mut m = Machine::new(t);
        m.on(Event::PttDown, t);
        m.tick(at(30, t));
        assert_eq!(m.on(Event::PttUp, at(31, t)), Effect::None);
        assert_eq!(m.state(), State::Thinking);
    }

    #[test]
    fn failure_then_retry_and_idle_ignores_stray_events() {
        let t = Instant::now();
        let mut m = Machine::new(t);
        assert_eq!(m.on(Event::PttUp, t), Effect::None);
        assert_eq!(m.on(Event::ReplyReady, t), Effect::None);
        m.on(Event::PttDown, t);
        m.on(Event::Failed, t);
        assert_eq!(m.state(), State::Error);
        assert_eq!(m.on(Event::PttDown, t), Effect::StartCapture);
    }
}
