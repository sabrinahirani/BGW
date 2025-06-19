use ark_bn254::Fr;
use ark_ff::PrimeField;
use ark_ff::Field;
use ark_ff::BigInteger;
use tokio::time::{timeout, Duration};

use std::collections::HashMap;
use tokio::sync::{mpsc, Barrier};
use std::sync::Arc;

use crate::sharing::{shamir_share, shamir_reconstruct, Share};
use crate::circuit::{Circuit, GateType};
use crate::message::Message;

/// A party participating in the BGW protocol
pub struct Party {
    pub id: usize,
    pub n: usize,
    pub t: usize,
    pub shares: HashMap<usize, Share>, // wire_id → Share
    pub tx: HashMap<usize, mpsc::Sender<Message>>, // recipient → Sender<Message>
    pub rx: mpsc::Receiver<Message>, // centralized inbox
    pub barrier: Arc<Barrier>, // barrier for synchronization
}

impl Party {
    /// Input Phase: share your inputs and receive others' inputs
    pub async fn input_phase(&mut self, circuit: &Circuit, inputs: &HashMap<usize, Fr>) {
        let input_wires = circuit.input_wires_by_owner(self.id);

        // Share owned inputs
        for &wire_id in &input_wires {
            let secret = *inputs.get(&wire_id).expect("Missing input value!");
            let shares = shamir_share(secret, self.t, self.n);

            for (pid, &share) in shares.iter().enumerate() {
                let msg = Message::InputShare(wire_id, share);
                if pid == self.id {
                    self.shares.insert(wire_id, share);
                } else {
                    let tx = self.tx.get_mut(&pid).unwrap();
                    tx.send(msg).await.expect("Failed to send input share");
                }
            }
        }

        // Receive inputs from other parties
        let expected = circuit.gates.iter()
            .filter(|g| matches!(g.gate_type, GateType::Input) && g.owner != Some(self.id))
            .count();

        let mut received = 0;
        while received < expected {
            if let Some(Message::InputShare(wire_id, share)) = self.rx.recv().await {
                self.shares.insert(wire_id, share);
                received += 1;
            }
        }
    }

    /// Evaluate circuit using received and computed shares
    pub async fn evaluate_circuit(&mut self, circuit: &Circuit) {
        for gate_id in circuit.topological_order() {
            let gate = &circuit.gates[gate_id];
            match gate.gate_type {
                GateType::Input => {
                    assert!(self.shares.contains_key(&gate.id), "Missing input share for wire {}", gate.id);
                }
                GateType::Add => {
                    self.eval_add(gate.id, gate.left.unwrap(), gate.right.unwrap());
                }
                GateType::ConstMul(c) => {
                    self.eval_const_mul(gate.id, gate.left.unwrap(), c);
                }
                GateType::Mul => {
                    let out = gate.id;
                    let left = gate.left.unwrap();
                    let right = gate.right.unwrap();
                    self.eval_mul(out, left, right).await;
                }
                GateType::Output => {
                    let input_wire = gate.left.unwrap();
                    let share = self.shares[&input_wire];
                    self.shares.insert(gate.id, share);
                }
            }
        }
    }

    /// Output Phase: exchange output shares and reconstruct result
    pub async fn output_phase(&mut self, output_wires: &[usize]) -> HashMap<usize, Fr> {
        let mut collected: HashMap<usize, Vec<Share>> = HashMap::new();

        for &wire_id in output_wires {
            let share = self.shares[&wire_id];
            for (&pid, tx) in &mut self.tx {
                if pid != self.id {
                    tx.send(Message::OutputShare(wire_id, share)).await.expect("Failed to send output share");
                }
            }
            collected.entry(wire_id).or_default().push(share);
        }

        while collected.values().any(|v| v.len() < self.t + 1) {
            if let Some(Message::OutputShare(wire_id, share)) = self.rx.recv().await {
                collected.entry(wire_id).or_default().push(share);
            }
        }

        collected.into_iter()
            .map(|(wire_id, shares)| (wire_id, shamir_reconstruct(&shares)))
            .collect()
    }

    fn eval_add(&mut self, out: usize, a: usize, b: usize) {
        let s1 = self.shares[&a];
        let s2 = self.shares[&b];
        assert_eq!(s1.x, s2.x, "Mismatched x-values in addition");

        self.shares.insert(out, Share {
            x: s1.x,
            value: s1.value + s2.value,
        });
    }

    fn eval_const_mul(&mut self, out: usize, a: usize, c: Fr) {
        let s = self.shares[&a];
        self.shares.insert(out, Share {
            x: s.x,
            value: s.value * c,
        });
    }

    pub async fn eval_mul(&mut self, out: usize, a: usize, b: usize) {
        let s1 = self.shares[&a];
        let s2 = self.shares[&b];
        assert_eq!(s1.x, s2.x, "Mismatched x values for multiplication");
    
        // Step 1: Compute local product (degree 2t)
        let local_product = Share {
            x: s1.x,
            value: s1.value * s2.value,
        };
    
        // Step 2: Broadcast product shares to all other parties
        for (&pid, tx) in &mut self.tx {
            if pid != self.id {
                tx.send(Message::MulShare(out, local_product))
                    .await
                    .expect("Failed to send multiplication share");
            }
        }
    
        // Step 3: Collect at least 2t + 1 distinct shares (including own)
        let mut shares = vec![local_product];
        while shares.len() < 2 * self.t + 1 {
            if let Some(Message::MulShare(wire_id, share)) = self.rx.recv().await {
                if wire_id == out && !shares.iter().any(|s| s.x == share.x) {
                    shares.push(share);
                }
            }
        }
    
        // Step 4: Reconstruct the product value
        let product_value = shamir_reconstruct(&shares);
        println!("Party {} reconstructed product value: {}", self.id, product_value);
    
        // Step 5: Reshare using Shamir (degree t)
        let resharing_shares = shamir_share(product_value, self.t, self.n);
    
        // Step 6: Send each share to the corresponding party
        for (&pid, tx) in &mut self.tx {
            let share = resharing_shares[pid]; // intended for pid
            if pid != self.id {
                tx.send(Message::Reshare(out, share))
                    .await
                    .expect("Failed to send resharing share");
            }
        }
    
        // Step 7: Receive resharing shares addressed to this party (same x each time)
        let my_x = Fr::from((self.id + 1) as u64);
        let mut final_shares = vec![resharing_shares[self.id]]; // include own
        while final_shares.len() < self.n {
            match timeout(Duration::from_secs(10), self.rx.recv()).await {
                Ok(Some(Message::Reshare(wire_id, share)))
                    if wire_id == out && share.x == my_x =>
                {
                    if !final_shares.iter().any(|s| s.x == share.x && s.value == share.value) {
                        final_shares.push(share);
                    }
                }
                Ok(Some(_)) => {} // unrelated message, ignore
                Ok(None) => {
                    println!("Party {}: channel closed unexpectedly!", self.id);
                    break;
                }
                Err(_) => {
                    println!("Party {}: Timeout waiting for resharing shares!", self.id);
                    break;
                }
            }
        }
    
        assert_eq!(
            final_shares.len(),
            self.n,
            "Party {} did not receive all resharing shares for wire {}",
            self.id,
            out
        );
    
        // ✅ Step 8: Sum values (since all have same x, different random masking)
        let sum: Fr = final_shares.iter().map(|s| s.value).sum();
        let inv_n = Fr::from(self.n as u64).inverse().unwrap(); // make sure n ≠ 0 mod p
        let my_share_value = sum * inv_n;
    
        self.shares.insert(
            out,
            Share {
                x: my_x,
                value: my_share_value,
            },
        );
    
    }
    
    
}
