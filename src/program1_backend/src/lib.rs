use candid::{CandidType, Decode, Deserialize, Encode};
use ic_cdk::{caller, query, update};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    {BoundedStorable, DefaultMemoryImpl,StableBTreeMap, Storable, Cell},
};
use serde::Serialize;
use std::{borrow::Cow, cell::RefCell};

type Memory = VirtualMemory<DefaultMemoryImpl>;
type IdCell = Cell<u64, Memory>;
const MAX_VALUE_SIZE: u32 = 5000;

#[derive(CandidType, Deserialize, Serialize, Clone)]
enum Choice {
    Approve,
    Reject,
    Pass,
}

#[derive(CandidType, Deserialize, Serialize)]
enum VoteError {
    AlreadyVoted,
    ProposalIsNotActive,
    NoSuchProposal,
    AccessRejected,
    UpdateError(String), // Improved error message
}

#[derive(CandidType, Deserialize, Clone, Serialize)]
struct Proposal {
    id: u64,
    description: String,
    approve: u32,
    reject: u32,
    pass: u32,
    is_active: bool,
    voted: Vec<candid::Principal>,
    owner: candid::Principal,
}

#[derive(CandidType, Deserialize, Clone, Serialize)]
struct CreateProposal {
    description: String,
    is_active: bool,
}

impl Storable for Proposal {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

impl BoundedStorable for Proposal {
    const MAX_SIZE: u32 = MAX_VALUE_SIZE;
    const IS_FIXED_SIZE: bool = false;
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static ID_COUNTER: RefCell<IdCell> = RefCell::new(
        IdCell::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))), 0)
            .expect("Cannot create a counter")
    );

    static PROPOSAL_MAP: RefCell<StableBTreeMap<u64, Proposal, Memory>> = RefCell::new(
        StableBTreeMap::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(1))))
    );
}

#[query]
fn get_proposal(id: u64) -> Option<Proposal> {
    PROPOSAL_MAP.with(|p| p.borrow().get(&id))
}

#[update]
fn create_proposal(proposal: CreateProposal) -> Option<Proposal> {
    let id = ID_COUNTER
    .with(|counter| {
        let current_value = *counter.borrow().get();
        counter.borrow_mut().set(current_value + 1)
    })
    .expect("cannot increment id counter");
    let value: Proposal = Proposal {
        id,
        description: proposal.description,
        approve: 0u32,
        reject: 0u32,
        pass: 0u32,
        is_active: proposal.is_active,
        voted: vec![],
        owner: caller(),
    };

    PROPOSAL_MAP.with(|p| p.borrow_mut().insert(id, value));
    Some(PROPOSAL_MAP.with(|p| p.borrow().get(&id).unwrap()))
}


#[update]
fn end_proposal(id: u64) -> Result<(), VoteError> {
    let result = PROPOSAL_MAP.with(|p| {
        let proposal_opt: Option<Proposal> = p.borrow().get(&id);
        let mut proposal = proposal_opt.ok_or(VoteError::NoSuchProposal)?;

        if caller() != proposal.owner {
            return Err(VoteError::AccessRejected);
        };

        proposal.is_active = false;

        p.borrow_mut().insert(id, proposal).ok_or(VoteError::UpdateError("Insert failed".to_string()))
    });

    if result.is_ok() {
        Ok(())
    }else {
        return Err(result.err().unwrap())
    }

}

#[update]
fn vote(id: u64, choice: Choice) -> Result<(), VoteError> {
    let result = PROPOSAL_MAP.with(|p| {
        let proposal_opt: Option<Proposal> = p.borrow().get(&id);
        let mut proposal = proposal_opt.ok_or(VoteError::NoSuchProposal)?;

        let caller = caller();

        if proposal.voted.contains(&caller) {
            return Err(VoteError::AlreadyVoted);
        } else if !proposal.is_active {
            return Err(VoteError::ProposalIsNotActive);
        };

        match choice {
            Choice::Approve => proposal.approve += 1,
            Choice::Reject => proposal.reject += 1,
            Choice::Pass => proposal.pass += 1,
        };

        proposal.voted.push(caller);

        p.borrow_mut().insert(id, proposal).ok_or(VoteError::UpdateError("Insert failed".to_string()))
    });
    if result.is_ok() {
        Ok(())
    }else {
        return Err(result.err().unwrap())
    }
}

// need this to generate candid
ic_cdk::export_candid!();