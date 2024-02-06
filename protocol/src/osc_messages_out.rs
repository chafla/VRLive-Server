// these pertain to messages that we are sending out from the server
// and will be prefixed /server


pub enum ServerMessage {
    Scene(SceneMessage),
    Timing,
    Performer,
    Backing(BackingMessage)
}

pub enum SceneMessage {
    State(u32)
}

pub enum BackingMessage {
    Stop,
    Start,
    /// Load a new backing track with the given descriptor
    New(String),
}

pub enum PerformerMessage {
    Ready(bool),
    Toggle(PerformerToggle),
    MatchMake(MatchMakeMessage)
}


pub enum MatchMakeMessage {
    Request
}

pub enum PerformerToggle {
    Audio(bool),
    Actor(bool),
    Motion(bool),
}