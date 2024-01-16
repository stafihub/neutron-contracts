use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("NotExist")]
    NotExist {},
    
    #[error("Relayers len not match")]
    RelayersLenNotMatch {},

    #[error("Invalid address")]
    InvalidAddress {},

    #[error("Duplicate")]
    Duplicate {},
    
    #[error("Already executed")]
    AlreadyExecuted {},
}
