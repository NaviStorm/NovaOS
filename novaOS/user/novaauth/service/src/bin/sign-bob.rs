#![no_std]
#![no_main]
mod common;
#[allow(dead_code)]
#[path = "../../../../../abi/files.rs"]
mod files;
#[allow(dead_code)]
#[path = "../../../../storage/runtime.rs"]
mod runtime;
use common::*;
use ed25519_dalek::{Signer, SigningKey};
use runtime as rt;
const SEED: &[u8; 32] = include_bytes!(env!("NOVA_LAB_BOB"));
include!("common/client_api.rs");
include!("common/policy_signer.rs");
