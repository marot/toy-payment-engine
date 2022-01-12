use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::{BufReader};
use std::sync::Arc;

use csv::{ByteRecord, ReaderBuilder, Writer};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use client_state::ClientState;
use operation::Operation;
use read_num_lines::read_num_lines;

use crate::client_state::ClientStateCsv;

mod client_state;
mod operation;
mod read_num_lines;
mod serialize_fractional;

struct ClientHandles {
    client_work: HashMap<u16, JoinHandle<ClientState>>,
}

impl ClientHandles {
    pub fn new() -> ClientHandles {
        ClientHandles {
            client_work: HashMap::new(),
        }
    }
}

impl ClientHandles {
    // Wait for the client state futures and write the result as csv to the writer.
    pub async fn serialize_work<W: io::Write>(&mut self, writer: &mut Writer<W>) {
        for work in self.client_work.drain() {
            match work.1.await {
                Ok(result) => {
                    let csv_data: ClientStateCsv = result.into();
                    if let Err(err) = writer.serialize(csv_data) {
                        eprintln!("Failed to serialize to csv with: {:?}", err);
                    }
                }
                Err(err) => {
                    eprintln!(
                        "Failed to wait for client state computation to finish with error {}",
                        err
                    )
                }
            }
        }
    }
}

// Parse the data as csv into a vector of operations.
fn parse_csv<R: io::Read>(read: R, chunk_size: usize) -> Vec<Operation> {
    let mut operations: Vec<Operation> = Vec::with_capacity(chunk_size);
    let mut record = ByteRecord::new();
    let mut trimmed = ByteRecord::new();

    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .buffer_capacity(512)
        .from_reader(read);

    for _ in 0..chunk_size {
        match reader.read_byte_record(&mut record) {
            Ok(true) => {
                // This is a custom function (see `csv.patch`) because `ByteRecord::trim` will
                // allocate memory. This version will re-use the memory similar to `ByteRecord::read_byte_record`.
                record.trim_noalloc(&mut trimmed);
                match trimmed.deserialize(None) {
                    Ok(operation) => {
                        operations.push(operation);
                    }
                    Err(err) => {
                        eprintln!("Failed to deserialize csv record with error {}.", err);
                    }
                };
            }
            Ok(false) => {
                // No more data available.
                break;
            }
            Err(err) => {
                eprintln!("Failed to read csv record with error {}.", err);
            }
        }
    }

    operations
}

const EXPECTED_OPERATIONS_PER_CLIENT: usize = 1024 * 100;

fn split_into_client_operations(operations: &mut Vec<Operation>) -> HashMap<u16, Vec<Operation>> {
    let mut client_operations: HashMap<u16, Vec<Operation>> = HashMap::new();
    operations.drain(..).for_each(|operation| {
        let client_ops = client_operations.entry(operation.client);
        let ops = match client_ops {
            Entry::Occupied(ops) => ops.into_mut(),
            Entry::Vacant(v) => v.insert(Vec::with_capacity(EXPECTED_OPERATIONS_PER_CLIENT)),
        };
        ops.push(operation);
    });

    client_operations
}

async fn spawn_for_each_client(
    world: Arc<Mutex<ClientHandles>>,
    client_operations: &mut HashMap<u16, Vec<Operation>>,
) {
    let mut world = world.lock().await;

    for (client, mut operations) in client_operations.drain() {
        let prior_work = world.client_work.remove(&client);
        let future = tokio::spawn(async move {
            // Wait for the client state computed based on a prior batch.
            let mut client_state = if let Some(work) = prior_work {
                work.await.expect("Failed to compute client state")
            } else {
                // or initialize a new one if this is the first batch for this client state.
                ClientState::new(client)
            };

            operations.drain(..).for_each(|operation| {
                client_state.apply_operation(operation);
            });

            client_state
        });

        world.client_work.insert(client, future);
    }
}

// Split the incoming operations into operations per-client and spawn the futures returning the client state.
async fn perform_work(operations: &mut Vec<Operation>, world: Arc<Mutex<ClientHandles>>) {
    let mut client_operations = split_into_client_operations(operations);

    spawn_for_each_client(world, &mut client_operations).await;
}

// Deserialize the data and spawn the per-client futures.
async fn parse_and_compute(
    world: Arc<Mutex<ClientHandles>>,
    data: Vec<u8>,
    chunk_size: usize,
    last_handle: Option<JoinHandle<()>>,
) {
    let mut operations = parse_csv(&data[..], chunk_size);

    if let Some(prio_task) = last_handle {
        if let Err(err) = prio_task.await {
            eprintln!("Failed to wait for prior task with {:?}", err);
        }
    }

    perform_work(&mut operations, world).await;
}

// Read the csv file in `filename`, process the operations and write the resulting client state into the passed `writer`.
pub async fn read_file_and_output_to_writer<W: io::Write>(
    filename: &str,
    writer: &mut Writer<W>,
    chunk_size: Option<usize>,
) -> io::Result<()> {
    let file = File::open(filename)?;
    // We split the incoming csv data into multiple parts, each having `lines_per_batch` lines.
    let lines_per_batch = chunk_size.unwrap_or(1024 * 1024 * 10);

    let mut reader = BufReader::with_capacity(lines_per_batch * 50, file);

    // Read header
    let mut header = Vec::with_capacity(50);
    // Read the first line - the header and discard it, we won't need it.
    read_num_lines(&mut reader, 1, &mut header).expect("Failed to parse csv header");

    // Store the handle to the last task. After parsing the csv data, we compute the operations for
    // each client. Each batch will spawn a future for each client. Because the order of operations
    // is important, the second batch will wait for the first batch to spawn the futures for each client
    // before spawning the futures itself.
    let mut last_task_handle: Option<JoinHandle<()>> = None;

    // Stores the futures that will return the client state for each client.
    let client_handles = Arc::new(Mutex::new(ClientHandles::new()));

    loop {
        let mut data = Vec::with_capacity(lines_per_batch * 50);
        match read_num_lines(&mut reader, lines_per_batch, &mut data) {
            Ok(read) => {
                if read == 0 {
                    break;
                }
            }
            Err(err) => {
                eprintln!("Received an error while reading from file: {}", err);
                break;
            }
        }

        last_task_handle = Some(tokio::spawn(parse_and_compute(
            client_handles.clone(),
            data,
            lines_per_batch,
            last_task_handle.take(),
        )))
    }

    if let Some(handle) = last_task_handle.take() {
        if let Err(err) = handle.await {
            eprintln!(
                "Failed to wait for last task to finish. Data may be incomplete: {:?}",
                err
            );
        }
    }

    let mut world = client_handles.lock().await;

    world.serialize_work(writer).await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::BufWriter;

    use super::*;

    async fn run_payment_engine(filename: &str, results: &[&str]) {
        let mut buf = BufWriter::new(Vec::new());
        {
            let mut writer = Writer::from_writer(&mut buf);
            read_file_and_output_to_writer(filename, &mut writer, Some(1))
                .await
                .expect("Failed to compute file");
            writer.flush().unwrap();
        }

        let bytes = buf.into_inner().unwrap();
        let string = String::from_utf8(bytes).unwrap();

        for result in results.iter() {
            assert!(string.contains(result))
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_lock_account() {
        run_payment_engine(
            "lock-account.csv",
            &["client,available,held,total,locked\n0,-55.5000,0.0,-55.5000,true\n"],
        )
        .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_resolved_dispute() {
        run_payment_engine(
            "resolved-dispute.csv",
            &["client,available,held,total,locked\n0,44.5000,0.0,44.5000,false\n"],
        )
        .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_three_clients() {
        run_payment_engine(
            "three-clients.csv",
            &[
                "client,available,held,total,locked",
                "2,-1.0,98.0,97.0,false",
                "0,99.0,0.0,99.0,false",
                "1,-1.0,99.0,98.0,false",
            ],
        )
        .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_whitespace() {
        run_payment_engine(
            "with-whitespace.csv",
            &[
                "client,available,held,total,locked",
                "0,100.0,0.0,100.0,false",
            ],
        )
        .await;
    }
}
