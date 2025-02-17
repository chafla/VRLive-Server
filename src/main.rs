use clap::{command, Parser};
use log::LevelFilter;
use simplelog;
use simplelog::{ColorChoice, CombinedLogger, Config, TerminalMode, TermLogger};
use protocol::handshake::ServerPortMap;

use server::server::{Server};

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
    port: u16
}


#[tokio::main]
async fn main() {

    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Debug, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
        ]
    ).unwrap();

    // gstreamer::init().unwrap();

    let args = Args::parse();
    let port_map = ServerPortMap {
        new_connections: args.port,
        ..Default::default()
    };
    // server::server::Portmap
    let mut server = Server::new(
        args.host, port_map
    );

    let _ = server.start().await;

    // let sample_osc_message = OSCMess
    println!("Hello, world!");
}
