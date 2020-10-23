#![cfg_attr(feature = "strict", deny(warnings))]

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};

pub mod accounts;
pub mod error;
pub mod instruction;
