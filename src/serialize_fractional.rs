use serde::{Serialize, Serializer};

pub fn serialize_fractional<S>(value: &i64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let integer_part = value / 10000;
    let fractional_part = (value % 10000).abs();
    format!("{}.{}", integer_part, fractional_part).serialize(serializer)
}
