use anyhow::Result;
use serde::Serialize;

pub fn print<T: Serialize>(json: bool, value: &T, human: impl FnOnce(&T) -> String) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(value)?);
    } else {
        println!("{}", human(value));
    }
    Ok(())
}
