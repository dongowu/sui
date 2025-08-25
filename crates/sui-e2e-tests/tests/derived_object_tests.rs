// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_macros::*;
use sui_test_transaction_builder::publish_package;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::derived_object::derive_object_id;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::object::Owner;
use sui_types::storage::WriteKind;
use sui_types::transaction::{CallArg, ObjectArg, Transaction};
use test_cluster::{TestCluster, TestClusterBuilder};

#[sim_test]
async fn derived_object_create_then_transfer_and_finally_receive() {
    let mut env = TestEnvironment::new().await;
    let parent = env.new_parent().await;

    let derived = env.new_derived(parent, 0u64, false).await;

    // Transfer the `any_obj` into the derived object's address
    let any_obj = env.new_any_obj(derived.0.into()).await;

    // Success -- we were able to "receive" an object transferred to our derived addr.
    let (_, owner) = env.receive(derived, any_obj).await;

    // The owner of the derived obj must now be the sender (since we received it and self-receive transfers to
    // ctx.sender()).
    assert_eq!(owner, Owner::AddressOwner(env.sender()))
}

#[sim_test]
async fn derived_object_claim_then_receive_already_transferred_object() {
    let env = TestEnvironment::new().await;
    let parent = env.new_parent().await;

    let derived_calculated_id = derive_object_id(
        parent.0,
        &sui_types::TypeTag::U64,
        &bcs::to_bytes(&0u64).unwrap(),
    )
    .unwrap();

    // Create a new object and transfer to our "derived" address before we have created
    // that derived address.
    let any_obj = env.new_any_obj(derived_calculated_id.into()).await;

    eprintln!("parent: {:?}", parent);
    eprintln!("any_obj: {:?}", any_obj);
    eprintln!("derived_calculated_id: {:?}", derived_calculated_id);

    // If we are able to claim & receive, good :)
    env.claim_and_receive(parent, 0u64, any_obj).await;
}

#[sim_test]
async fn derived_object_claim_and_add_df_in_one_tx() {
    let env = TestEnvironment::new().await;
    let parent = env.new_parent().await;

    let _derived = env.new_derived(parent, 0u64, true).await;
}

fn get_created_object(fx: &TransactionEffects, id: Option<ObjectID>) -> ObjectRef {
    let obj = fx
        .all_changed_objects()
        .iter()
        .find(|obj| {
            obj.2 == WriteKind::Create && (id.is_none() || id.is_some_and(|id| obj.0 .0 == id))
        })
        .unwrap()
        .clone();

    eprintln!("obj: {:?}", obj);

    obj.0
}

struct TestEnvironment {
    pub test_cluster: TestCluster,
    move_package: ObjectID,
}

impl TestEnvironment {
    async fn new() -> Self {
        let test_cluster = TestClusterBuilder::new().build().await;

        let move_package = publish_move_package(&test_cluster).await.0;

        Self {
            test_cluster,
            move_package,
        }
    }

    async fn create_move_call(
        &self,
        function: &'static str,
        arguments: Vec<CallArg>,
    ) -> Transaction {
        let transaction = self
            .test_cluster
            .test_transaction_builder()
            .await
            .move_call(self.move_package, "derived", function, arguments)
            .build();
        self.test_cluster
            .wallet
            .sign_transaction(&transaction)
            .await
    }

    async fn move_call(
        &self,
        function: &'static str,
        arguments: Vec<CallArg>,
    ) -> anyhow::Result<(TransactionEffects, TransactionEvents)> {
        let transaction = self.create_move_call(function, arguments).await;
        self.test_cluster
            .execute_transaction_return_raw_effects(transaction)
            .await
    }

    // Create a new `Parent` object
    async fn new_parent(&self) -> ObjectRef {
        let (fx, _) = self.move_call("create_parent", vec![]).await.unwrap();
        assert!(fx.status().is_ok());

        // Find the only created object that has to be the "parent" we created.
        get_created_object(&fx, None)
    }

    // Create a new `AnyObj` object (treated as a "random" object)
    async fn new_any_obj(&self, recipient: SuiAddress) -> ObjectRef {
        let arguments = vec![CallArg::Pure(recipient.to_vec())];

        let (fx, _) = self.move_call("create_any_obj", arguments).await.unwrap();
        assert!(fx.status().is_ok());

        // Find the only created object that has to be the "any_obj" we created.
        get_created_object(&fx, None)
    }

    // Create a new `Derived` object.
    // If `with_df` is true, the derived object will have a dynamic field added to it, for testing purposes
    // (mainly to test that the fresh object does not get into "modified" state.)
    async fn new_derived(&self, parent: ObjectRef, key: u64, with_df: bool) -> ObjectRef {
        let arguments = vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(parent)),
            CallArg::Pure(key.to_le_bytes().to_vec()),
        ];
        let (fx, _) = self
            .move_call(
                if with_df {
                    "create_derived_with_df"
                } else {
                    "create_derived"
                },
                arguments,
            )
            .await
            .unwrap();
        assert!(fx.status().is_ok());

        let derived_id = derive_object_id(
            parent.0,
            &sui_types::TypeTag::U64,
            &bcs::to_bytes(&key).unwrap(),
        )
        .unwrap();

        get_created_object(&fx, Some(derived_id))
    }

    async fn claim_and_receive(&self, parent: ObjectRef, key: u64, child: ObjectRef) {
        let arguments: Vec<CallArg> = vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(parent)),
            CallArg::Pure(key.to_le_bytes().to_vec()),
            CallArg::Object(ObjectArg::Receiving(child)),
        ];

        let (fx, _) = self
            .move_call("claim_and_receive", arguments)
            .await
            .unwrap();
        assert!(fx.status().is_ok());

        // We should have mutated:
        // 1. The parent object
        // 2. The derived object
        // 3. The "tto"'d object.
        assert_eq!(fx.mutated_excluding_gas().len(), 3);
    }

    async fn receive(&self, derived: ObjectRef, child: ObjectRef) -> (ObjectID, Owner) {
        let arguments = vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(derived)),
            CallArg::Object(ObjectArg::Receiving(child)),
        ];

        let (fx, _) = self.move_call("receive", arguments).await.unwrap();

        assert!(fx.status().is_ok());

        // Find the "child" object we received.
        let obj = fx
            .all_changed_objects()
            .iter()
            .find(|obj| obj.0 .0 == child.0)
            .cloned()
            .unwrap();

        (obj.0 .0, obj.1)
    }

    fn sender(&mut self) -> SuiAddress {
        self.test_cluster.wallet.active_address().unwrap()
    }
}

async fn publish_move_package(test_cluster: &TestCluster) -> ObjectRef {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    publish_package(&test_cluster.wallet, path).await
}
