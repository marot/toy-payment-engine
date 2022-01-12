use crate::serialize_fractional::serialize_fractional;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::Debug;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[repr(u16)]
pub enum OperationType {
    #[serde(rename = "deposit")]
    Deposit,
    #[serde(rename = "withdrawal")]
    Withdrawal,
    #[serde(rename = "dispute")]
    Dispute,
    #[serde(rename = "resolve")]
    Resolve,
    #[serde(rename = "chargeback")]
    Chargeback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operation {
    pub type_: OperationType,
    pub client: u16,
    pub tx_id: u32,
    #[serde(
        deserialize_with = "deserialize_amount",
        serialize_with = "serialize_fractional"
    )]
    pub amount: i64,
}

#[cfg(test)]
impl Operation {
    pub fn deposit(client: u16, tx_id: u32, amount: i64) -> Operation {
        Operation {
            type_: OperationType::Deposit,
            client,
            tx_id,
            amount,
        }
    }

    pub fn withdrawal(client: u16, tx_id: u32, amount: i64) -> Operation {
        Operation {
            type_: OperationType::Withdrawal,
            client,
            tx_id,
            amount,
        }
    }

    pub fn dispute(client: u16, tx_id: u32) -> Operation {
        Operation {
            type_: OperationType::Dispute,
            client,
            tx_id,
            amount: 0,
        }
    }

    pub fn resolve(client: u16, tx_id: u32) -> Operation {
        Operation {
            type_: OperationType::Resolve,
            client,
            tx_id,
            amount: 0,
        }
    }

    pub fn chargeback(client: u16, tx_id: u32) -> Operation {
        Operation {
            type_: OperationType::Chargeback,
            client,
            tx_id,
            amount: 0,
        }
    }
}

fn deserialize_amount<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;

    let mut parts = s.split('.');

    let mut amount: i64;

    let integer_part = parts
        .next()
        .ok_or_else(|| D::Error::custom("Failed to parse integer part."))?
        .parse::<u32>()
        .map_err(|_| D::Error::custom("Failed to parse integer part."))?;
    amount = (integer_part as i64) * 10000;
    if let Some(fractional) = parts.next() {
        let fractional_part = format!("{:0<4}", fractional)
            .parse::<u16>()
            .map_err(|_| D::Error::custom("Failed to parse fractional part."))?;
        amount += fractional_part as i64;
    }

    Ok(amount)
}

#[cfg(test)]
mod tests {
    use csv::{ReaderBuilder, WriterBuilder};
    use std::io::BufWriter;

    use super::*;

    #[test]
    fn test_deserialize_operation() {
        let buf = "Operation,ClientId,TxId,Amount
deposit,1,2,1.0
withdrawal,2,3,5.0
dispute,3,4,1.2340
resolve,5,6,1.3333
chargeback,7,8,4294967295.9999";
        let mut reader = ReaderBuilder::new().from_reader(buf.as_bytes());
        let operations: Vec<Operation> = reader
            .byte_records()
            .map(|record| record.unwrap().deserialize(None).unwrap())
            .collect();

        assert_eq!(operations.len(), 5);
        assert_eq!(
            operations,
            vec![
                Operation {
                    type_: OperationType::Deposit,
                    client: 1,
                    tx_id: 2,
                    amount: 10000
                },
                Operation {
                    type_: OperationType::Withdrawal,
                    client: 2,
                    tx_id: 3,
                    amount: 50000
                },
                Operation {
                    type_: OperationType::Dispute,
                    client: 3,
                    tx_id: 4,
                    amount: 12340
                },
                Operation {
                    type_: OperationType::Resolve,
                    client: 5,
                    tx_id: 6,
                    amount: 13333
                },
                Operation {
                    type_: OperationType::Chargeback,
                    client: 7,
                    tx_id: 8,
                    amount: 42949672959999
                },
            ]
        );
    }

    #[test]
    fn test_serialize_operation() {
        let operations = vec![
            Operation {
                type_: OperationType::Deposit,
                client: 1,
                tx_id: 2,
                amount: 10000,
            },
            Operation {
                type_: OperationType::Withdrawal,
                client: 2,
                tx_id: 3,
                amount: 50000,
            },
            Operation {
                type_: OperationType::Dispute,
                client: 3,
                tx_id: 4,
                amount: 12340,
            },
            Operation {
                type_: OperationType::Resolve,
                client: 5,
                tx_id: 6,
                amount: -13333,
            },
            Operation {
                type_: OperationType::Chargeback,
                client: 7,
                tx_id: 8,
                amount: 42949672959999,
            },
        ];

        let mut buf = BufWriter::new(Vec::new());

        {
            let mut writer = WriterBuilder::new().from_writer(&mut buf);
            for operation in operations.iter() {
                writer.serialize(operation).unwrap();
            }
        }

        let result = "type_,client,tx_id,amount
deposit,1,2,1.0
withdrawal,2,3,5.0
dispute,3,4,1.2340
resolve,5,6,-1.3333
chargeback,7,8,4294967295.9999
";

        let bytes = buf.into_inner().unwrap();
        let string = String::from_utf8(bytes).unwrap();

        assert_eq!(string.as_str(), result);
    }
}
