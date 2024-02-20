use gstreamer::Pipeline;
use gstreamer as gst;
use gstreamer::prelude::*;
use anyhow::Error;


/// A WebRTC listener
struct GSTListener {
    listener_pipeline: Pipeline,

}

impl GSTListener {

    pub fn build() -> Result<Self, Error> {
        let pipeline = Pipeline::with_name("Audio Listener");

        let webrtc_sink = gst::ElementFactory::make("webrtcsink").build()?;
    
        Ok(Self {
            listener_pipeline: pipeline,
        })
    }
    pub fn get_pipeline(&self) -> &Pipeline {
        &self.listener_pipeline
    }

    pub fn get_pipeline_mut(&mut self) -> &mut Pipeline {
        &mut self.listener_pipeline
    }
}