use crate::light_validation::{storage_proof::StorageProof, LightValidation};
use pfx_api::storage_sync::{BlockValidator, Error as SyncError, Result};
use std::string::ToString;

pub use storage_ext::ChainStorage;

impl BlockValidator for LightValidation<chain::Runtime> {
    fn submit_finalized_headers(
        &mut self,
        bridge_id: u64,
        header: chain::Header,
        ancestry_proof: Vec<chain::Header>,
        grandpa_proof: Vec<u8>,
    ) -> Result<()> {
        self.submit_finalized_headers(bridge_id, header, ancestry_proof, grandpa_proof)
            .map_err(|e| SyncError::HeaderValidateFailed(e.to_string()))
    }

    fn validate_storage_proof(
        &self,
        state_root: chain::Hash,
        proof: StorageProof,
        items: &[(&[u8], &[u8])],
    ) -> Result<()> {
        self.validate_storage_proof(state_root, proof, items)
            .map_err(|e| SyncError::StorageProofFailed(e.to_string()))
    }
}

mod storage_ext {
    use crate::chain;
    use pfx_mq::{Message, MessageOrigin};
    use pfx_trie_storage::TrieStorage;
    use serde::{Deserialize, Serialize};
    use sp_core::H256;
    use sp_state_machine::{Ext, OverlayedChanges};

    #[derive(Serialize, Deserialize, Default)]
    pub struct ChainStorage {
        trie_storage: TrieStorage<crate::RuntimeHasher>,
    }

    impl Clone for ChainStorage {
        fn clone(&self) -> Self {
            Self { trie_storage: self.trie_storage.snapshot() }
        }
    }

    impl From<TrieStorage<crate::RuntimeHasher>> for ChainStorage {
        fn from(value: TrieStorage<crate::RuntimeHasher>) -> Self {
            Self { trie_storage: value }
        }
    }

    impl ChainStorage {
        pub fn from_pairs(pairs: impl Iterator<Item = (impl AsRef<[u8]>, impl AsRef<[u8]>)>) -> Self {
            let mut me = Self::default();
            me.load(pairs);
            me
        }

        pub fn snapshot(&self) -> Self {
            Self { trie_storage: self.trie_storage.snapshot() }
        }

        pub fn load(&mut self, pairs: impl Iterator<Item = (impl AsRef<[u8]>, impl AsRef<[u8]>)>) {
            self.trie_storage.load(pairs);
        }

        pub fn root(&self) -> &sp_core::H256 {
            self.trie_storage.root()
        }

        pub fn inner(&self) -> &TrieStorage<crate::RuntimeHasher> {
            &self.trie_storage
        }

        pub fn inner_mut(&mut self) -> &mut TrieStorage<crate::RuntimeHasher> {
            &mut self.trie_storage
        }

        pub fn execute_with<R>(&self, f: impl FnOnce() -> R) -> R {
            let backend = self.trie_storage.as_trie_backend();
            let mut overlay = OverlayedChanges::default();
            let mut ext = Ext::new(&mut overlay, backend, None);
            sp_externalities::set_and_run_with_externalities(&mut ext, f)
        }

        pub fn mq_messages(&self) -> Vec<Message> {
            self.execute_with(chain::CesMq::messages)
        }

        pub fn timestamp_now(&self) -> chain::Moment {
            self.execute_with(chain::Timestamp::now)
        }

        /// Get the next mq sequnce number for given sender. Default to 0 if no message sent.
        pub fn mq_sequence(&self, sender: &MessageOrigin) -> u64 {
            self.execute_with(|| pfx_pallet_mq::OffchainIngress::<chain::Runtime>::get(sender))
                .unwrap_or(0)
        }

        /// Return `None` if given pflix hash is not allowed on-chain
        pub(crate) fn get_pflix_bin_added_at(&self, runtime_hash: &H256) -> Option<chain::BlockNumber> {
            self.execute_with(|| pfx_pallet_tee_worker::PflixBinAddedAt::<chain::Runtime>::get(runtime_hash))
        }

        pub fn is_master_key_first_holder(&self, worker_pubkey: &pfx_types::WorkerPublicKey) -> bool {
            self.execute_with(|| {
                pfx_pallet_tee_worker::MasterKeyFirstHolder::<chain::Runtime>::get()
                    .map_or_else(|| false, |e| e == *worker_pubkey)
            })
        }

        pub(crate) fn is_worker_registered(&self, worker: &pfx_types::WorkerPublicKey) -> bool {
            self.execute_with(|| pfx_pallet_tee_worker::Workers::<chain::Runtime>::get(worker))
                .is_some()
        }

        pub(crate) fn minimum_pflix_version(&self) -> (u32, u32, u32) {
            self.execute_with(pfx_pallet_tee_worker::MinimumPflixVersion::<chain::Runtime>::get)
        }

        pub(crate) fn is_pflix_bin_in_whitelist(&self, measurement: &H256) -> bool {
            let list = self.execute_with(pfx_pallet_tee_worker::PflixBinAllowList::<chain::Runtime>::get);
            list.contains(measurement)
        }
    }
}