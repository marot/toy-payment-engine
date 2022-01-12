use std::env;
use std::io::stdout;
use std::vec::Vec;

use csv::Writer;
use tokio::io;

use payment_engine::*;

#[tokio::main]
async fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let filename = args
        .get(1)
        .expect("At least one argument required (The csv filename)");
    let mut writer = Writer::from_writer(stdout());
    if let Err(err) = read_file_and_output_to_writer(filename.as_str(), &mut writer, None).await {
        eprintln!("Failed to run payment engine with {}", err);
    }
    Ok(())
}
