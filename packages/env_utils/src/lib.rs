#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use thiserror::Error;

#[derive(Clone, Copy, Debug)]
pub enum ParseIntError {
    InvalidDigit,
}

const fn parse_byte(b: u8, pow10: u128) -> Result<u128, ParseIntError> {
    let r = b.wrapping_sub(48);

    if r > 9 {
        Err(ParseIntError::InvalidDigit)
    } else {
        Ok((r as u128) * pow10)
    }
}

pub(crate) const POW10: [u128; 20] = {
    let mut array = [0; 20];
    let mut current: u128 = 1;

    let mut index = 20;

    loop {
        index -= 1;
        array[index] = current;

        if index == 0 {
            break;
        }

        current *= 10;
    }

    array
};

pub const fn parse(b: &str) -> Result<usize, ParseIntError> {
    let bytes = b.as_bytes();

    let mut result: usize = 0;

    let len = bytes.len();

    // Start at the correct index of the table,
    // (skip the power's that are too large)
    let mut index_const_table = POW10.len().wrapping_sub(len);
    let mut index = 0;

    while index < b.len() {
        let a = bytes[index];
        let p = POW10[index_const_table];

        let r = match parse_byte(a, p) {
            Err(e) => return Err(e),
            Ok(d) => d,
        };

        result = result.wrapping_add(r as usize);

        index += 1;
        index_const_table += 1;
    }

    Ok(result)
}

#[derive(Error, Debug)]
pub enum EnvUsizeError {
    #[error(transparent)]
    Var(#[from] std::env::VarError),
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
}

pub fn env_usize(name: &str) -> Result<usize, EnvUsizeError> {
    Ok(std::env::var(name)?.parse::<usize>()?)
}

#[macro_export]
macro_rules! env_usize {
    ($name:expr $(,)?) => {
        match $crate::parse(env!($name)) {
            Ok(v) => v,
            Err(_e) => panic!("Environment variable not set"),
        }
    };
}

#[derive(Error, Debug)]
pub enum DefaultEnvUsizeError {
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
}

pub fn default_env_usize(name: &str, default: usize) -> Result<usize, DefaultEnvUsizeError> {
    match std::env::var(name) {
        Ok(value) => Ok(value.parse::<usize>()?),
        Err(_) => Ok(default),
    }
}

#[macro_export]
macro_rules! default_env_usize {
    ($name:expr, $default:expr $(,)?) => {
        match $crate::option_env_usize!($name) {
            Some(v) => v,
            None => $default,
        }
    };
}

#[derive(Error, Debug)]
pub enum OptionEnvUsizeError {
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
}

pub fn option_env_usize(name: &str) -> Result<Option<usize>, OptionEnvUsizeError> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value.parse::<usize>()?)),
        Err(_) => Ok(None),
    }
}

#[macro_export]
macro_rules! option_env_usize {
    ($name:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => match $crate::parse(v) {
                Ok(v) => Some(v),
                Err(_e) => panic!("Invalid environment variable value"),
            },
            None => None,
        }
    };
}

#[macro_export]
macro_rules! option_env_u32 {
    ($name:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => match $crate::parse(v) {
                Ok(v) => Some(v as u32),
                Err(_e) => panic!("Invalid environment variable value"),
            },
            None => None,
        }
    };
}

pub fn default_env(name: &str, default: &str) -> String {
    match std::env::var(name) {
        Ok(value) => value,
        Err(_) => default.to_string(),
    }
}

#[macro_export]
macro_rules! default_env {
    ($name:expr, $default:expr $(,)?) => {
        match option_env!($name) {
            Some(v) => v,
            None => $default,
        }
    };
}
