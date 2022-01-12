# Toy payment engine

The last time I worked on Rust was more than half a year ago - and then only part time for a couple of weeks. My naming 
regarding async code is most likely off - and it took me some time to get up to speed again. I did enjoy working with 
Rust more than I remembered, and I have to admit that I could have probably finished earlier if improving the code
would not have been that much fun...

## Specification notes
One aspect was unclear to me
- **Can both deposits and withdrawals be disputed?**
I decided that only deposits can be disputed. If withdrawals could be disputed, a user could
deposit 100 units, withdraw 100 units, dispute the withdrawal and the available funds would
be back at 100, allowing the user to withdraw again.

### Precision
We have at most 4 decimals. That means we can multiply by 10000 and store the amount as u64. We can't use floats
because of loss of information. The maximum size of an amount is `u32 * 10000`. I do not handle the case
where we have an overflow. We store the funds as `i64` because they can turn negative if a deposit is disputed.

## Basics
The payment engine can be run with `cargo run -- lock-account.csv`. I decided against modifying the 
dev profile to have release flags included, so `cargo run --release -- lock-account.csv` will be much faster.

## Completeness
I took plenty of time on this - so I do hope I did not miss anything crucial :)

I did not test error cases extensively.

## Correctness
- The serialization/deserialization of operations has unit tests, the serialization of the client state has no unit tests,
because it re-uses the serialization of operations.
- The `read_num_lines` includes unit tests, as it is a very complex part.
- Computing the client state also includes unit tests. It is basically a state machine
so there are many different cases that should be tested. If the client state would have more states (than frozen, transactions in dispute)
I would have made the state machine explicit to simplify the individual parts/transitions.
- `lib.rs` includes some integration tests using real csv files.

The async code is only tested in the integration tests. I tested by splitting each line into its own future. My assumption
is that if errors exist, they are most likely related to how the work is split up (one off errors etc.) - so running
each line in its own future would show errors in how the work is split up.

My usual approach (that would take too much time here) is to use a property testing library to randomly generate test input, then
run the entire integration test with the test input with multiple batch sizes (1, 100, 10000) and verify that the result is always the same.

I used proptest to generate a test data set but removed it again because it is not part of the challenge.

I also tested manually with a big (6.5 GB) file.

## Safety and Robustness
- I only used `unwrap` in test code.
- `except` is used where I assume something really went wrong (e.g. await failing)
- Otherwise, errors are logged, and we try to continue (e.g. `src/lib.rs:79`)

At some point I used `memmap` to access the file directly. After reading more about it, it looked like
it could cause UB when someone else would modify the file while we read it. While some workarounds exist (change file ownership, ...)
I decided to remove `memmap` to avoid increasing complexity.

Everything else should be safe, but I did not test the error cases extensively, but I did not test the error cases extensively, but I did not test the error cases extensively, but I did not test the error cases extensively.

## Efficiency
The current setup only allows one csv input. The csv is split into multiple parts that will be deserialized independently in a job. This is done while streaming the input csv. Then the operations of each batch
are computed one after another. We start computing the operations while we are still deserializing the csv, because computing the operations is very fast compared to deserializing.
Otherwise, only one core would be busy in the end after deserializing, reducing the throughput (I am not 100% sure about this, because this approach
might lead to more cache misses).

The operations are split per-client and computed in a separate future that represents the clients state (For each client there is one future returning the final state).

With 8 logical processors (i7-e700K) I was able to compute ~6.5GB in ~32s.

If each csv data source represents a different disjunctive set of clients, the above pipeline can be run in parallel too. Anything else
doesn't make sense as far as I am aware, because we could not determine the correct order of operations otherwise.
We only lock once in `src/lib.rs:114` to spawn the per-client jobs (because they need to store the client future globally). If the disjunctive
set of clients is known in advance, we can also have one Mutex for each set of clients.

## Maintainability
We have some problems here:
- `read_num_lines`: This is basically a re-implementation of `read_until` with some specialized behaviour. It tries to avoid calling `fill_buf` too often and instead
fills the buffer (that should be quite big) and then iterates through x amount of lines.
This was originally written for the `memmap` version of the code, where I would only compute offsets instead of filling the buffer with the read data.
Now the behaviour is very similar to `read_until`. I tried to replace it with `read_until` but the performance hit was too high.
- Patching the rust-csv library.
See `csv.patch`. I copied the `trim` function and re-implemented it to avoid allocation by passing a `trimmed` `ByteRecord`. Patching libraries is something
that is not too hard in the npm ecosystem. In the Rust ecosystem I would have forked the library, but I did not have enough time to properly set everything up.
Sorry for adding an entire library to the repo...

