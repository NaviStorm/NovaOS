//! Contrat réservé à un futur gestionnaire. Aucune cible ne le met encore en œuvre.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalAddress(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtualAddress(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    ReadOnly,
    ReadWrite,
    ReadExecute,
}

/// Opérations sur une page d’un espace d’adressage. Une adresse non alignée,
/// un mappage existant ou un démappage absent doivent produire une erreur.
pub trait VirtualMemory {
    type Error;
    const PAGE_BYTES: usize;

    /// # Safety
    /// L’appelant possède le cadre et garantit sa durée de vie, la cohérence de
    /// ses alias et la validité des références Rust pendant tout le mappage.
    unsafe fn map(
        &mut self,
        page: VirtualAddress,
        frame: PhysicalAddress,
        access: Access,
    ) -> Result<(), Self::Error>;

    /// # Safety
    /// La page doit avoir cessé d’être utilisée par le code, les piles et les
    /// références actives. Le succès implique l’invalidation des traductions.
    unsafe fn unmap(&mut self, page: VirtualAddress) -> Result<PhysicalAddress, Self::Error>;
}
