use crate::{CryptVersion, KeyKind};

/// Erros de descriptografia. Nenhuma variante carrega material de chave.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("chave inválida: {0}")]
    InvalidKey(&'static str),

    #[error("arquivo de chave não reconhecido (esperado `key` do crypt14 ou `encrypted_backup.key` do crypt15)")]
    UnknownKeyFile,

    #[error("cabeçalho do backup inválido: {0}")]
    Header(&'static str),

    #[error("formato não suportado: o arquivo não é crypt14 nem crypt15")]
    Unsupported,

    #[error("a chave é do tipo {key:?}, mas o backup é {file:?}")]
    KeyMismatch { key: KeyKind, file: CryptVersion },

    #[error("chave incorreta ou arquivo corrompido")]
    Authentication,

    #[error("arquivo truncado")]
    Truncated,

    #[error("falha ao descomprimir o conteúdo: {0}")]
    Decompress(std::io::Error),

    #[error("conteúdo descomprimido excede o limite de {0} bytes")]
    TooLarge(u64),
}
