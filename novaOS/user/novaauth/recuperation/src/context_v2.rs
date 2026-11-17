//! Contexte public et collecte à deux destinataires de rôle.
use super::*;

pub const RECOVERY_V2_BYTES: usize = 376;

#[derive(Clone)]
pub struct RecoveryContextV2([u8; RECOVERY_V2_BYTES]);

impl RecoveryContextV2 {
    /// La validité cryptographique et courante de l'approbation est vérifiée
    /// séparément, avant d'utiliser ce contexte pour une remise.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let data: [u8; RECOVERY_V2_BYTES] = bytes.try_into().map_err(|_| Error::Format)?;
        if &data[..8] != b"NARECV02" {
            return Err(Error::Format);
        }
        for (offset, size) in [
            (8, 16),
            (24, 16),
            (40, 16),
            (56, 32),
            (88, 32),
            (136, 32),
            (168, 32),
            (208, 32),
            (248, 32),
            (280, 32),
            (312, 32),
            (344, 32),
        ] {
            if data[offset..offset + size].iter().all(|&value| value == 0) {
                return Err(Error::Scope);
            }
        }
        for offset in [120, 128, 200, 240] {
            let value = u64_at(&data, offset);
            if value > i64::MAX as u64 || (offset != 120 && value == 0) {
                return Err(Error::Limit);
            }
        }
        if data[56..88] == data[88..120]
            || data[248..280] == data[280..312]
            || data[248..280] == data[312..344]
            || data[280..312] == data[312..344]
        {
            return Err(Error::Scope);
        }
        let possession = VerifyingKey::from_bytes(data[312..344].try_into().unwrap())
            .map_err(|_| Error::Crypto)?;
        if possession.is_weak() {
            return Err(Error::Crypto);
        }
        Ok(Self(data))
    }
    pub fn bytes(&self) -> &[u8; RECOVERY_V2_BYTES] {
        &self.0
    }
    pub fn digest(&self) -> [u8; 32] {
        hash(&self.0)
    }
    pub fn manifest(&self) -> [u8; 32] {
        self.0[168..200].try_into().unwrap()
    }
    pub fn epoch(&self) -> u64 {
        u64_at(&self.0, 200)
    }
    pub fn recipient(&self, role: u8) -> Result<[u8; 32]> {
        let offset = 248 + 32 * role_index(role)?;
        Ok(self.0[offset..offset + 32].try_into().unwrap())
    }
    pub fn possession(&self) -> [u8; 32] {
        self.0[312..344].try_into().unwrap()
    }
    pub fn delivery(&self, delivery: [u8; 16], nonce: [u8; 32]) -> DeliveryContext {
        DeliveryContext {
            recovery: self.digest(),
            manifest: self.manifest(),
            epoch: self.epoch(),
            delivery,
            nonce,
        }
    }
}

/// Le contexte et les deux clés de signature sont épinglés par l'admission.
/// Chaque clé de réception reste attachée à son rôle pendant toute l'opération.
pub struct SharesV2 {
    recovery: [u8; 32],
    manifest: [u8; 32],
    epoch: u64,
    recipients: [[u8; 32]; 2],
    senders: [VerifyingKey; 2],
    values: [Option<Secret>; 2],
    digests: [Option<[u8; 32]>; 2],
}

impl SharesV2 {
    pub fn new(context: &RecoveryContextV2, senders: [VerifyingKey; 2]) -> Result<Self> {
        if senders[0] == senders[1] || senders.iter().any(VerifyingKey::is_weak) {
            return Err(Error::Scope);
        }
        Ok(Self {
            recovery: context.digest(),
            manifest: context.manifest(),
            epoch: context.epoch(),
            recipients: [context.recipient(1)?, context.recipient(2)?],
            senders,
            values: [None, None],
            digests: [None, None],
        })
    }
    pub fn receive(
        &mut self,
        role: u8,
        context: &DeliveryContext,
        key: &PrivateKey,
        envelope: &[u8],
    ) -> Result<bool> {
        let index = role_index(role)?;
        if context.recovery != self.recovery
            || context.manifest != self.manifest
            || context.epoch != self.epoch
            || public(key) != self.recipients[index]
        {
            return Err(Error::Scope);
        }
        let value = open_share(role, context, key, &self.senders[index], envelope)?;
        let digest = hash(envelope);
        if let Some(previous) = self.digests[index] {
            return if previous == digest {
                Ok(false)
            } else {
                Err(Error::Duplicate)
            };
        }
        self.values[index] = Some(value);
        self.digests[index] = Some(digest);
        Ok(true)
    }
    /// Une seule consommation est possible. La vérification courante de
    /// l'autorité et de la tête précède cet appel, puis l'activation finale.
    pub fn take(mut self) -> Result<[Secret; 2]> {
        Ok([
            self.values[0].take().ok_or(Error::Missing)?,
            self.values[1].take().ok_or(Error::Missing)?,
        ])
    }
}
