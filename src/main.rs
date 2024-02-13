use clap::{command, Parser};
use server::Server;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    name: String,
    #[arg(short, long)]
    host: String,
    #[arg(short, long)]
    port: u16
}


fn main() {
    let args = Args::parse();
    // let server = Server::new()

    // let sample_osc_message = OSCMess
    println!("Hello, world!");
}
