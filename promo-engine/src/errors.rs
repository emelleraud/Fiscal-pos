use thiserror::Error;

#[derive(Debug, Error)]
pub enum PromoError {
    #[error("Panier vide — impossible d'évaluer les promotions")]
    EmptyCart,
    #[error(
        "Promotion {id} sans valeur définie (value_cents et value_bps sont tous les deux null)"
    )]
    MissingValue { id: uuid::Uuid },
}
