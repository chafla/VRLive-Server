

// Tracking for events.
// Unless stated otherwise, these are evaluated per second.
#[derive(Clone, Debug)]
pub enum AnalyticsData {
    ClientEvents(u16),
    AudienceMocapMessages(u16),
    Throughput(ThroughputAnalytics)
}

#[derive(Copy, Clone, Debug)]
pub enum ThroughputAnalytics {
    AudienceMocapBytesIn(u16),
    AudienceOSCBytesIn(u16),
    Audience
}

#[derive(Copy, Clone, Debug)]
pub enum InternalAnalytics {

}



