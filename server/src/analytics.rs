// Tracking for events.
// Unless stated otherwise, these are evaluated per second.
#[derive(Clone, Debug)]
pub enum AnalyticsData {
    ClientEvents(u16),
    AudienceMocapMessages(u16),
    Throughput(ThroughputAnalytics),
    Client(ClientAnalytics)
}

#[derive(Copy, Clone, Debug)]
pub enum ThroughputAnalytics {
    AudienceMocapBytesIn(usize),
    AudienceOSCBytesIn(usize),
    PerformerOSCBytesIn(usize),
    Audience
}

#[derive(Copy, Clone, Debug)]
pub enum ClientAnalytics {
    
}

#[derive(Copy, Clone, Debug)]
pub enum InternalAnalytics {

}


