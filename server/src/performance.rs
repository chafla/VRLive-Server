

#[allow(dead_code)]
// TODO actually implement this

#[derive(Clone, Debug, Default)]
pub struct BackingTrackState {
    /// Whether or not we've sent a single backing track out to users
    sent: bool,
    /// Whether or not the backing track is currently playing for our users.
    playing: bool,
    /// Title of the backing track that we've sent out to our users
    title: String
}

impl BackingTrackState {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }
    
    pub fn is_playing(&self) -> bool {
        self.playing
    }
    
    pub fn mark_sent(&mut self) {
        self.sent = true;
    }
    
    pub fn mark_playing(&mut self, playing: bool) {
        self.playing = playing;
    }
}

#[derive(Clone, Copy, Debug, Default)]
enum PerformanceStatus {
    #[default]
    /// Waiting for a performer to join
    AwaitingPerformer,
    /// Performers are in, but are not ready
    PerformersNotReady,
    /// Performers are in, and have readied up, awaiting start.
    PerformersReady,
    /// A performance is currently ongoing.
    Performing,
}

#[derive(Clone, Debug, Default)]
/// Struct storing different information about the state of the performance.
/// This doesn't need to be accessed too often, so it's okay to wrap this in a mutex.
pub struct PerformanceState {
    /// The backing track that is currently playing.
    current_backing_track: Option<BackingTrackState>,
    /// Any previous backing tracks that we've played.
    previous_backing_tracks: Vec<BackingTrackState>,
    /// The current state of the performance.
    performance_status: PerformanceStatus
}

impl PerformanceState {
    pub fn backing_track(&self) -> Option<&BackingTrackState> {
        if let Some(bt) = &self.current_backing_track {
            Some(bt)
        }
        else {
            None
        }
    }
    
    pub fn backing_track_mut(&mut self) -> Option<&mut BackingTrackState> {
        if let Some(bt) = &mut self.current_backing_track {
            Some(bt)
        }
        else {
            None
        }
    }
    
    pub fn update_backing_track(&mut self, track_name: &str) {
        if self.current_backing_track.is_some() {
            self.previous_backing_tracks.push(self.current_backing_track.take().unwrap())
        }
        
        self.current_backing_track = Some(BackingTrackState::new(track_name));
    }
}

