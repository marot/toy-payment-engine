use std::collections::HashMap;

use serde::Serialize;

use crate::operation::{Operation, OperationType};
use crate::serialize_fractional::serialize_fractional;

#[derive(Debug, PartialEq, Clone)]
enum TransactionStatus {
    Normal,
    Disputed,
}

#[derive(Debug, PartialEq, Clone)]
enum ClientStatus {
    Normal,
    Frozen,
}

#[derive(Debug, PartialEq, Clone)]
struct Transaction {
    operation: Operation,
    status: TransactionStatus,
}

#[derive(Debug, PartialEq, Clone)]
pub struct ClientState {
    client: u16,
    available: i64,
    held: i64,
    status: ClientStatus,

    transactions: HashMap<u32, Transaction>,
}

#[derive(Serialize)]
pub struct ClientStateCsv {
    client: u16,
    #[serde(serialize_with = "serialize_fractional")]
    available: i64,
    #[serde(serialize_with = "serialize_fractional")]
    held: i64,
    #[serde(serialize_with = "serialize_fractional")]
    total: i64,
    locked: bool,
}

impl From<ClientState> for ClientStateCsv {
    fn from(state: ClientState) -> Self {
        ClientStateCsv {
            client: state.client,
            locked: state.status == ClientStatus::Frozen,
            available: state.available,
            held: state.held,
            total: state.available + state.held,
        }
    }
}

impl ClientState {
    pub fn new(client: u16) -> ClientState {
        ClientState {
            client,
            available: 0,
            held: 0,
            status: ClientStatus::Normal,
            transactions: HashMap::new(),
        }
    }
}

impl ClientState {
    pub fn apply_operation(&mut self, operation: Operation) {
        if self.status == ClientStatus::Frozen {
            return;
        }

        match operation.type_ {
            OperationType::Deposit => {
                self.available += operation.amount;
                self.transactions.insert(
                    operation.tx_id,
                    Transaction {
                        operation,
                        status: TransactionStatus::Normal,
                    },
                );
            }
            OperationType::Withdrawal => {
                if self.available >= operation.amount {
                    self.available -= operation.amount;
                }
            }
            OperationType::Dispute => {
                if let Some(tx) = self.transactions.get_mut(&operation.tx_id) {
                    if tx.status != TransactionStatus::Disputed {
                        self.available -= tx.operation.amount;
                        self.held += tx.operation.amount;
                        tx.status = TransactionStatus::Disputed;
                    }
                }
            }
            OperationType::Resolve => {
                if let Some(tx) = self.transactions.get_mut(&operation.tx_id) {
                    if tx.status == TransactionStatus::Disputed {
                        self.available += tx.operation.amount;
                        self.held -= tx.operation.amount;
                        tx.status = TransactionStatus::Normal;
                    }
                }
            }
            OperationType::Chargeback => {
                if let Some(tx) = self.transactions.get_mut(&operation.tx_id) {
                    if tx.status == TransactionStatus::Disputed {
                        self.held -= tx.operation.amount;
                        self.status = ClientStatus::Frozen;
                        tx.status = TransactionStatus::Normal;
                    }
                }
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::client_state::ClientStatus;
    use crate::{ClientState, Operation};

    fn frozen_account() -> ClientState {
        let mut client_state = ClientState::new(0);
        vec![
            Operation::deposit(0, 1, 10),
            Operation::deposit(0, 2, 10),
            Operation::withdrawal(0, 3, 5),
            Operation::dispute(0, 2),
            Operation::chargeback(0, 2),
        ]
        .drain(..)
        .for_each(|operation| client_state.apply_operation(operation));

        client_state
    }

    #[test]
    fn test_frozen_ignores_operations() {
        let mut frozen = frozen_account();
        assert_eq!(frozen.status, ClientStatus::Frozen);
        let original = frozen.clone();
        frozen.apply_operation(Operation::deposit(0, 4, 10));
        assert_eq!(frozen, original);
        frozen.apply_operation(Operation::withdrawal(0, 5, 5));
        assert_eq!(frozen, original);
        frozen.apply_operation(Operation::dispute(0, 1));
        assert_eq!(frozen, original);
        frozen.apply_operation(Operation::resolve(0, 2));
        assert_eq!(frozen, original);
        frozen.apply_operation(Operation::chargeback(0, 2));
    }

    #[test]
    fn test_cant_withdraw_more_than_available() {
        let mut client = ClientState::new(0);
        client.apply_operation(Operation::deposit(0, 1, 25));
        client.apply_operation(Operation::deposit(0, 2, 25));
        {
            let state = client.clone();
            let mut client = client.clone();

            client.apply_operation(Operation::withdrawal(0, 3, 51));
            // Operation was not applied!
            assert_eq!(state, client);
            client.apply_operation(Operation::withdrawal(0, 3, 50));
            assert_eq!(client.available, 0);
        }

        {
            let mut client = client.clone();
            client.apply_operation(Operation::dispute(0, 1));
            let state = client.clone();
            client.apply_operation(Operation::withdrawal(0, 4, 26));
            // Operation was not applied!
            assert_eq!(state, client);
            client.apply_operation(Operation::withdrawal(0, 4, 25));
            assert_eq!(client.available, 0);
        }
    }

    #[test]
    fn test_can_only_dispute_existing_transactions() {
        let mut client = ClientState::new(0);
        client.apply_operation(Operation::deposit(0, 1, 25));
        let state = client.clone();
        client.apply_operation(Operation::dispute(0, 2));
        // Operation was not applied!
        assert_eq!(state, client);
    }

    #[test]
    fn test_resolving_is_inverse_of_dispute() {
        let mut client = ClientState::new(0);
        client.apply_operation(Operation::deposit(0, 1, 25));
        let state = client.clone();
        client.apply_operation(Operation::dispute(0, 1));
        assert_ne!(state, client);
        client.apply_operation(Operation::resolve(0, 1));
        // State after resolving a dispute is equal to the state before the dispute (if no operations are inbetween)
        assert_eq!(state, client);
    }

    #[test]
    fn test_can_only_resolve_disputes() {
        let mut client = ClientState::new(0);
        client.apply_operation(Operation::deposit(0, 1, 25));
        let state = client.clone();
        client.apply_operation(Operation::resolve(0, 2));
        // Operation was not applied!
        assert_eq!(state, client);
    }

    #[test]
    fn test_dispute_can_only_be_applied_once() {
        let mut client = ClientState::new(0);
        client.apply_operation(Operation::deposit(0, 1, 25));
        client.apply_operation(Operation::dispute(0, 1));
        let state = client.clone();
        client.apply_operation(Operation::dispute(0, 1));
        // Operation was not applied!
        assert_eq!(state, client);
    }

    #[test]
    fn test_can_only_chargeback_disputes() {
        let mut client = ClientState::new(0);
        client.apply_operation(Operation::deposit(0, 1, 25));
        let state = client.clone();
        client.apply_operation(Operation::chargeback(0, 2));
        // Operation was not applied!
        assert_eq!(state, client);
    }
}
