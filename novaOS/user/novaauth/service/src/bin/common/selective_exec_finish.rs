//! Soumissions distinctes d'une tombe et d'un terminal après admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Action {
    Tombstone,
    Finish,
    Observe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Submission {
    Fresh,
    TombstoneSent,
    FinishSent,
}

impl Submission {
    pub(super) fn next(&mut self, selective_cancellation: bool) -> Action {
        match *self {
            Self::Fresh if selective_cancellation => {
                *self = Self::TombstoneSent;
                Action::Tombstone
            }
            Self::Fresh => {
                *self = Self::FinishSent;
                Action::Finish
            }
            Self::TombstoneSent | Self::FinishSent => Action::Observe,
        }
    }

    /// Une consultation signée a retrouvé l'admission en attente. La tombe
    /// ne peut plus prendre effet sur cet identifiant. FINISH reste un premier
    /// envoi distinct, même si la réponse de la tombe a été perdue.
    pub(super) fn finish_after_tombstone(&mut self) -> bool {
        if *self != Self::TombstoneSent {
            return false;
        }
        *self = Self::FinishSent;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, Submission};

    #[test]
    fn tombstone_and_finish_have_separate_send_states() {
        let mut state = Submission::Fresh;
        assert_eq!(state.next(true), Action::Tombstone);
        // Tentative de tombe puis plusieurs consultations sans preuve.
        assert_eq!(state.next(true), Action::Observe);
        assert_eq!(state.next(true), Action::Observe);
        // Un reçu lié au même identifiant établit une admission Pending.
        assert!(state.finish_after_tombstone());
        // La perte de FINISH impose une consultation, jamais un second envoi.
        assert_eq!(state.next(true), Action::Observe);
        assert!(!state.finish_after_tombstone());
    }

    #[test]
    fn normal_finish_is_only_sent_once() {
        let mut state = Submission::Fresh;
        assert_eq!(state.next(false), Action::Finish);
        assert_eq!(state.next(false), Action::Observe);
        assert!(!state.finish_after_tombstone());
    }
}
