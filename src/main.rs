use ark_bn254::Fr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Barrier};
use tokio::task;
use tokio::time::{timeout, Duration};

use bgw::circuit::{Circuit, GateType};
use bgw::party::Party;
use bgw::message::Message;

#[tokio::main]
async fn main() {
    let n = 5;
    let t = 2;

    // Build circuit: (a + b) * c
    let mut circuit = Circuit::new();

    let a = circuit.add_gate(GateType::Input, None, None, Some(0));
    let b = circuit.add_gate(GateType::Input, None, None, Some(1));
    let c = circuit.add_gate(GateType::Input, None, None, Some(2));

    let sum = circuit.add_gate(GateType::Add, Some(a), Some(b), None);
    let mul = circuit.add_gate(GateType::Mul, Some(sum), Some(c), None);
    let out = circuit.add_gate(GateType::Output, Some(mul), None, None);

    // Inputs: party 0 = 2, party 1 = 3, party 2 = 4
    let inputs: Vec<Fr> = vec![Fr::from(2u64), Fr::from(3u64), Fr::from(4u64)];

    println!("Inputs:");
    println!("Party 0: a = {}", inputs[0]);
    println!("Party 1: b = {}", inputs[1]);
    println!("Party 2: c = {}", inputs[2]);
    println!("Party 3: no input (helper)");
    println!("\nComputing arithmetic circuit...\n");

    // Channel setup
    let mut party_txs = vec![HashMap::new(); n]; // party_txs[i][j] = tx from i to j
    let mut inboxes = Vec::with_capacity(n);     // each party's central inbox

    let barrier = Arc::new(Barrier::new(n)); // Barrier for synchronization

    // For each party, create a central inbox (mpsc::Receiver) and a map of txs to all parties
    for to in 0..n {
        let (central_tx, central_rx) = mpsc::channel::<Message>(100);
        inboxes.push(central_rx);

        for from in 0..n {
            if from != to {
                let (tx, mut rx) = mpsc::channel::<Message>(100);
                party_txs[from].insert(to, tx.clone());
                let central_tx_clone = central_tx.clone();
                // Forward rx into central_tx
                task::spawn(async move {
                    while let Some(msg) = rx.recv().await {
                        if let Err(_) = central_tx_clone.send(msg).await {
                            // Channel closed, exit forwarding task
                            break;
                        }
                    }
                });
            }
        }
        // Add self-sender so each party can send to itself
        party_txs[to].insert(to, central_tx.clone());
    }

    // Launch parties
    let mut handles = vec![];

    for (pid, rx) in inboxes.into_iter().enumerate() {
        let circuit_clone = circuit.clone();
        let mut tx_map = party_txs[pid].clone();
        let barrier = barrier.clone();

        let inputs_map = if pid < 3 {
            let mut map = HashMap::new();
            let wire_id = circuit.input_wires_by_owner(pid)[0];
            map.insert(wire_id, inputs[pid]);
            map
        } else {
            HashMap::new()
        };

        handles.push(tokio::spawn(async move {
            let mut party = Party {
                id: pid,
                n,
                t,
                shares: HashMap::new(),
                tx: tx_map,
                rx,
                barrier,
            };

            party.input_phase(&circuit_clone, &inputs_map).await;
            party.evaluate_circuit(&circuit_clone).await;
            let output = party.output_phase(&[out]).await;

            if let Some(val) = output.get(&out) {
                println!("Party {} reconstructed output: {}", pid, val);
            } else {
                println!("Party {} failed to reconstruct output", pid);
            }
        }));
    }

    // Wait for all parties
    for (pid, h) in handles.into_iter().enumerate() {
        match h.await {
            Ok(_) => println!("Party {} completed successfully", pid),
            Err(e) => println!("Party {} failed: {:?}", pid, e),
        }
    }

    println!("\nVerification:");
    println!("Expected result: (2 + 3) * 4 = 20");
    println!("All parties should have computed the same result.");
}
