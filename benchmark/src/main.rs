extern crate clap;

use benchmark::*;
use clap::{App, Arg, SubCommand};
use faster_rs::FasterKv;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn main() {
    let matches = App::new("faster-rs Benchmark")
        .subcommand(
            SubCommand::with_name("process-ycsb")
                .about("Process YCSB file to extract key")
                .arg(
                    Arg::with_name("input")
                        .required(true)
                        .help("Path to input file"),
                )
                .arg(
                    Arg::with_name("output")
                        .required(true)
                        .help("Path to output file"),
                ),
        )
        .subcommand(
            SubCommand::with_name("run")
                .about("Run benchmark")
                .arg(
                    Arg::with_name("num-threads")
                        .short("n")
                        .required(true)
                        .takes_value(true)
                        .display_order(1)
                        .help("Number of threads to use"),
                )
                .arg(
                    Arg::with_name("load")
                        .required(true)
                        .help("Path to YCSB load keys"),
                )
                .arg(
                    Arg::with_name("run")
                        .required(true)
                        .help("Path to YCSB run keys"),
                )
                .arg(Arg::with_name("workload").required(true).possible_values(&[
                    "read_upsert_50_50",
                    "rmw_100",
                    "upsert_100",
                    "read_100",
                ])),
        )
        .subcommand(
            SubCommand::with_name("run-all")
                .about("Run benchmark with different thread configurations")
                .arg(
                    Arg::with_name("load")
                        .required(true)
                        .help("Path to YCSB load keys"),
                )
                .arg(
                    Arg::with_name("run")
                        .required(true)
                        .help("Path to YCSB run keys"),
                )
                .arg(Arg::with_name("workload").required(true).possible_values(&[
                    "read_upsert_50_50",
                    "rmw_100",
                    "upsert_100",
                    "read_100",
                ])),
        )
        .subcommand(
            SubCommand::with_name("larger-than-memory")
                .about("Run benchmark with a defined in-memory log size")
                .arg(
                    Arg::with_name("log-size")
                        .short("l")
                        .required(true)
                        .takes_value(true)
                        .display_order(1)
                        .help("Size of in-memory log (GB)"),
                )
                .arg(
                    Arg::with_name("load")
                        .required(true)
                        .help("Path to YCSB load keys"),
                )
                .arg(
                    Arg::with_name("run")
                        .required(true)
                        .help("Path to YCSB run keys"),
                ),
        )
        .subcommand(
            SubCommand::with_name("generate-keys")
                .about("Generate sequential keys")
                .arg(
                    Arg::with_name("load/run")
                        .required(true)
                        .takes_value(true)
                        .possible_values(&["load", "run"])
                        .help("Generate keys for load or run"),
                )
                .arg(
                    Arg::with_name("output")
                        .required(true)
                        .help("Path to output file"),
                ),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("process-ycsb") {
        let input = matches.value_of("input").expect("No input file specified");
        let output = matches
            .value_of("output")
            .expect("No output file specified");
        println!("Processing YCSB workload");
        process_ycsb(input, output);
    } else if let Some(matches) = matches.subcommand_matches("run") {
        let num_threads = matches
            .value_of("num-threads")
            .expect("Number of threads not specified");
        let num_threads: u8 = num_threads
            .parse()
            .expect("num-threads argument must be integer");
        let load_keys_file = matches
            .value_of("load")
            .expect("File containing load transactions not specified");
        let run_keys_file = matches
            .value_of("run")
            .expect("File containing run transactions not specified");
        let workload = matches
            .value_of("workload")
            .expect("Workload not specified");
        let op_allocator = match workload {
            "read_upsert_50_50" => read_upsert5050,
            "rmw_100" => rmw_100,
            "upsert_100" => upsert_100,
            "read_100" => read_100,
            _ => panic!("Unexpected workload specified. Options are: read_upsert_50_50, rmw_100"),
        };

        let table_size: u64 = 134217728;
        let log_size: u64 = 64 * 1024 * 1024 * 1024; // 64 GB
        let (load_keys, txn_keys) = load_files(load_keys_file, run_keys_file);
        let load_keys = Arc::new(load_keys);
        let txn_keys = Arc::new(txn_keys);

        let mut benchmark_results = Vec::with_capacity(4);

        for _ in 0..4 {
            let store = Arc::new(FasterKv::new_in_memory(table_size, log_size));
            println!("Populating datastore");
            populate_store(&store, &load_keys, 48);
            println!("Beginning benchmark");
            let result = run_benchmark(&store, &txn_keys, num_threads, op_allocator);
            benchmark_results.push(result);
        }
        println!(
            "{} threads: {:?} ops/second/thread",
            num_threads, benchmark_results
        );
    } else if let Some(matches) = matches.subcommand_matches("generate-keys") {
        let output_file = matches
            .value_of("output")
            .expect("Output file not specified");
        let workload = matches
            .value_of("load/run")
            .expect("Must specify load or run");
        println!("Generating sequential keys");
        generate_sequential_keys(output_file, workload);
    } else if let Some(matches) = matches.subcommand_matches("run-all") {
        let load_keys_file = matches
            .value_of("load")
            .expect("File containing load transactions not specified");
        let run_keys_file = matches
            .value_of("run")
            .expect("File containing run transactions not specified");
        let workload = matches
            .value_of("workload")
            .expect("Workload not specified");
        let op_allocator = match workload {
            "read_upsert_50_50" => read_upsert5050,
            "rmw_100" => rmw_100,
            "upsert_100" => upsert_100,
            "read_100" => read_100,
            _ => panic!("Unexpected workload specified. Options are: read_upsert_50_50, rmw_100"),
        };

        let table_size: u64 = 134217728;
        let log_size: u64 = 32 * 1024 * 1024 * 1024; // 32 GB
        let (load_keys, txn_keys) = load_files(load_keys_file, run_keys_file);
        let load_keys = Arc::new(load_keys);
        let txn_keys = Arc::new(txn_keys);

        let thread_configurations = vec![1, 2, 4, 8, 16, 32, 48];
        let mut benchmark_results = HashMap::new();

        for _ in 0..4 {
            for num_threads in &thread_configurations {
                let store = Arc::new(FasterKv::new_in_memory(table_size, log_size));
                println!("Populating datastore");
                populate_store(&store, &load_keys, 48);
                println!("Beginning benchmark");
                let result = run_benchmark(&store, &txn_keys, *num_threads, op_allocator);
                let entry = benchmark_results
                    .entry(num_threads)
                    .or_insert(Vec::with_capacity(4));
                entry.push(result);
            }
        }

        for (num_threads, result) in benchmark_results {
            println!("{} threads: {:?} ops/second/thread", num_threads, result);
        }
    } else if let Some(matches) = matches.subcommand_matches("larger-than-memory") {
        let log_size = matches
            .value_of("log-size")
            .expect("Number of threads not specified");
        let log_size: u64 = log_size.parse().expect("log-size argument must be integer");
        let load_keys_file = matches
            .value_of("load")
            .expect("File containing load transactions not specified");
        let run_keys_file = matches
            .value_of("run")
            .expect("File containing run transactions not specified");

        let table_size: u64 = 134217728;
        let log_size: u64 = log_size * 1024 * 1024 * 1024;
        let (load_keys, txn_keys) = load_files(load_keys_file, run_keys_file);
        let load_keys = Arc::new(load_keys);
        let txn_keys = Arc::new(txn_keys);

        let mut benchmark_results = Vec::with_capacity(4);

        for _ in 0..4 {
            let store = Arc::new(
                FasterKv::new(table_size, log_size, String::from("benchmark-store")).unwrap(),
            );
            println!("Populating datastore");
            populate_store(&store, &load_keys, 48);

            let done = Arc::new(AtomicBool::new(false));
            let done_clone = Arc::clone(&done);
            let store_clone = Arc::clone(&store);
            std::thread::spawn(move || {
                while !done_clone.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_secs(30));
                    println!(
                        "Checkpoint token : {}",
                        store_clone.checkpoint().unwrap().token
                    );
                }
            });

            println!("Beginning benchmark");
            let result = run_benchmark(&store, &txn_keys, 8, rmw_100);
            done.store(true, Ordering::SeqCst);
            benchmark_results.push(result);

            let _ = store.clean_storage();
        }
        println!("{} GB: {:?} ops/second/thread", log_size, benchmark_results);
    }
}
