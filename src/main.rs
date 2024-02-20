use clap::{command, Parser};
use log::LevelFilter;
use gstreamer;

use server::server::{PortMap, Server};
use simplelog;
use simplelog::{ColorChoice, CombinedLogger, Config, TerminalMode, TermLogger};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    /// The name we will be identifying ourselves as
    name: String,
    #[arg(long)]
    /// The hostname we will be broadcasting as
    host: String,
    #[arg(short, long)]
    /// Our main incoming connections port
    port: u16,
    #[arg(short, long)]
    /// Whether or not we want to use udp multicast
    multicast: bool
}


#[tokio::main]
async fn main() {

    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Debug, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
        ]
    ).unwrap();

    gstreamer::init().unwrap();

    let args = Args::parse();
    // server::server::Portmap
    let mut server = Server::new(
        args.host, PortMap::new(args.port), args.multicast
    );

    let _ = server.start().await;

    // let sample_osc_message = OSCMess
    println!("Hello, world!");
}
