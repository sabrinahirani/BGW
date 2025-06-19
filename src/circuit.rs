use ark_bn254::Fr;

/// supported gate types in the arithmetic circuit
#[derive(Clone, Debug)]
pub enum GateType {
    Input,
    Add,
    Mul,
    ConstMul(Fr),
    Output,
}

/// a gate in the arithmetic circuit
#[derive(Clone, Debug)]
pub struct Gate {
    pub id: usize,
    pub gate_type: GateType,
    pub left: Option<usize>,
    pub right: Option<usize>,
    pub owner: Option<usize>, // only applies to input wires (specifies party that owns the wire)
}

#[derive(Clone)]
pub struct Circuit {
    pub gates: Vec<Gate>,
}

impl Circuit {
    pub fn new() -> Self {
        Circuit {
            gates: Vec::new(),
        }
    }

    pub fn add_gate(&mut self, gate_type: GateType, left: Option<usize>, right: Option<usize>, owner: Option<usize>) -> usize {
        let id = self.gates.len();
        self.gates.push(Gate {id, gate_type, left, right, owner});
        id
    }

    pub fn input_wires_by_owner(&self, owner: usize) -> Vec<usize> {
        self.gates.iter()
            .filter(|g| matches!(g.gate_type, GateType::Input) && g.owner == Some(owner))
            .map(|g| g.id)
            .collect()
    }

    pub fn output_wires(&self) -> Vec<usize> {
        self.gates.iter()
            .filter(|g| matches!(g.gate_type, GateType::Output))
            .map(|g| g.id)
            .collect()
    }

    pub fn topological_order(&self) -> Vec<usize> {
        let mut visited = vec![false; self.gates.len()];
        let mut order = Vec::new();

        fn dfs(gate_id: usize, gates: &[Gate], visited: &mut [bool], order: &mut Vec<usize>) {
            if visited[gate_id] {
                return;
            }
            visited[gate_id] = true;

            let gate = &gates[gate_id];
            if let Some(left) = gate.left {
                dfs(left, gates, visited, order);
            }
            if let Some(right) = gate.right {
                dfs(right, gates, visited, order);
            }

            order.push(gate_id);
        }

        for i in 0..self.gates.len() {
            dfs(i, &self.gates, &mut visited, &mut order);
        }

        order
    }
}
