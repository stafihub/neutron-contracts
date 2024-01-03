use neutron_sdk::NeutronError;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ContractError {
    #[error("Unauthorized")]
    Unauthorized {},
}

impl From<ContractError> for NeutronError {
    fn from(error: ContractError) -> Self {
        NeutronError::Std(cosmwasm_std::StdError::generic_err(format!("{:?}", error)))
    }
}