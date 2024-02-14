use clap::{command, Parser};

use server::server::{PortMap, Server};

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
    let args = Args::parse();
    // server::server::Portmap
    let mut server = Server::new(
        args.host, PortMap::new(args.port),
    );

    let _ = server.start().await;

    // let sample_osc_message = OSCMess
    println!("Hello, world!");
}
