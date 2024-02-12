use clap::{command, Parser};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    name: String
}


fn main() {
    let args = Args::parse();

    // let sample_osc_message = OSCMess
    println!("Hello, world!");
}
