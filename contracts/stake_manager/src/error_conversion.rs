use cosmwasm_std::StdError;
use thiserror::Error;

use neutron_sdk::NeutronError;

#[derive(Debug, Error, PartialEq)]
pub enum ContractError {
    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Status not allow")]
    StatusNotAllow {},

    #[error("Pending share token not empty")]
    PendingShareNotEmpty {},

    #[error("Delegations not exist")]
    DelegationsNotExist {},

    #[error("Rate change over limit")]
    RateChangeOverLimit {},

    #[error("Encode error: {0}")]
    EncodeError(String),

    #[error("Validator for unbond not enough")]
    ValidatorForUnbondNotEnough {},

    #[error("Delegation submission height")]
    DelegationSubmissionHeight {},

    #[error("Withdraw Addr balances submission height")]
    WithdrawAddrBalanceSubmissionHeight {},

    #[error("Rebond height")]
    RebondHeight {},

    #[error("Pool is paused")]
    PoolIsPaused {},

    #[error("Already latest era")]
    AlreadyLatestEra {},

    #[error("Validator addresses list")]
    ValidatorAddressesListSize {},

    #[error("Rate is zero")]
    RateIsZero {},

    #[error("Instantiate2 address failed, err: {0}")]
    Instantiate2AddressFailed(String),

    #[error("Rate not match")]
    RateNotMatch {},

    #[error("Closed channel ID unmatch")]
    ClosedChannelIdUnmatch {},

    #[error("Era process not end")]
    EraProcessNotEnd {},

    #[error("Validator already exit")]
    ValidatorAlreadyExit {},

    #[error("Validators empty")]
    ValidatorsEmpty {},

    #[error("Old validator not exist")]
    OldValidatorNotExist {},

    #[error("New validator already exist")]
    NewValidatorAlreadyExist {},

    #[error("Tokens len not match")]
    TokensLenNotMatch {},

    #[error("Share token not exist")]
    ShareTokenNotExist {},

    #[error("Duplicate token")]
    DuplicateToken {},

    #[error("Invalid interchain account ID")]
    InvalidInterchainAccountId {},

    #[error("Interchain account ID already exist")]
    InterchainAccountIdAlreadyExist {},

    #[error("Counterparty version not match")]
    CounterpartyVersionNotMatch {},

    #[error("Can't parse counterparty version")]
    CantParseCounterpartyVersion {},

    #[error("LSM stake not support")]
    LsmStakeNotSupport {},

    #[error("LSM pending stake over limit")]
    LsmPendingStakeOverLimit {},

    #[error("Pool ICQ not updated")]
    PoolIcqNotUpdated {},

    #[error("Params error: funds not match")]
    ParamsErrorFundsNotMatch {},

    #[error("Less than minimal stake")]
    LessThanMinimalStake {},

    #[error("Less than minimal era seconds")]
    LessThanMinimalEraSeconds {},

    #[error("Denom not match")]
    DenomNotMatch {},

    #[error("Denom path not match")]
    DenomPathNotMatch {},

    #[error("Denom trace not match")]
    DenomTraceNotMatch {},

    #[error("Validator not support")]
    ValidatorNotSupport {},

    #[error("Token amount zero")]
    TokenAmountZero {},

    #[error("No validator info")]
    NoValidatorInfo {},

    #[error("Unsupported message: {0}")]
    UnsupportedMessage(String),

    #[error("Unsupported reply message id: {0}")]
    UnsupportedReplyId(u64),

    #[error("Encode error: LSD token amount is zero")]
    EncodeErrLsdTokenAmountZero {},

    #[error("Encode error: Unstake times limit reached")]
    EncodeErrUnstakeTimesLimitReached {},

    #[error("Encode error: Zero withdraw amount")]
    EncodeErrZeroWithdrawAmount {},

    #[error("Burn LSD token amount is zero")]
    BurnLsdTokenAmountIsZero {},

    #[error("Empty unstake list")]
    EmptyUnstakeList {},

    #[error("Unstake index: {0} pool not match")]
    UnstakeIndexPoolNotMatch(u64),

    #[error("Unstake index: {0} unstaker not match")]
    UnstakeIndexUnstakerNotMatch(u64),

    #[error("Unstake index: {0} status not match")]
    UnstakeIndexStatusNotMatch(u64),

    #[error("Unstake index: {0} not withdrawable")]
    UnstakeIndexNotWithdrawable(u64),

    #[error("ICQ error: Reply no result")]
    ICQErrReplyNoResult {},

    #[error("ICQ error: Failed to parse response: {0}")]
    ICQErrFailedParse(String),

    #[error("ICQ error: New key build failed")]
    ICQNewKeyBuildFailed {},

    #[error("Callback error: Sequence not found")]
    CallBackErrSequenceNotFound {},

    #[error("Callback error: Channel id not found")]
    CallBackErrChannelIDNotFound {},

    #[error("Callback error: Error message")]
    CallBackErrErrorMsg {},

    #[error("Period too small")]
    PeriodTooSmall {},
}

impl From<ContractError> for NeutronError {
    fn from(error: ContractError) -> Self {
        NeutronError::Std(StdError::generic_err(format!("{:?}", error)))
    }
}

impl From<ContractError> for StdError {
    fn from(error: ContractError) -> Self {
        StdError::generic_err(format!("{:?}", error))
    }
}
