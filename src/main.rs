use clap::{command, Parser};
use log::LevelFilter;

use server::server::{PortMap, Server};
use simplelog;
use simplelog::{ColorChoice, CombinedLogger, Config, SimpleLogger, TerminalMode, TermLogger};
use protocol::osc_messages_in::ClientMessage;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    name: String,
    #[arg(long)]
    host: String,
    #[arg(short, long)]
    port: u16
}


#[tokio::main]
async fn main() {

    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Debug, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
        ]
    ).unwrap();

    let args = Args::parse();
    // server::server::Portmap
    let mut server = Server::new(
        args.host, PortMap::new(args.port),
    );

    let _ = server.start().await;

    // let sample_osc_message = OSCMess
    println!("Hello, world!");
}
