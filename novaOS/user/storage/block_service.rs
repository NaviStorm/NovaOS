//! Admission commune des trames du service bloc, avant tout traitement du backend.
use crate::{runtime::*, wire};

pub(crate) fn request(input: u64, output: u64, last: u64, resident: bool) -> Result<ReceiveV1> {
    loop {
        let m = match receive_for(input, 5000) {
            Err(TIMED_OUT) if resident => continue,
            r => r?,
        };
        if m.kind == 1 && m.tag > last && m.len == 128 && wire::syntax(&m.payload) {
            return Ok(m);
        }
        send(output, m.tag, &[BAD_ARGUMENT as u8])?;
    }
}
