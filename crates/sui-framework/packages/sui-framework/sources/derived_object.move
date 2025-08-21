// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Enables the creation of objects with deterministic addresses derived from a parent object's UID.
/// This module provides a way to generate objects with predictable addresses based on a parent UID
/// and a key, creating a namespace that ensures uniqueness for each parent-key combination,
/// which is usually how registries are built.
///
/// Key features:
/// - Deterministic address generation based on parent object UID and key
/// - Derived objects can exist and operate independently of their parent
///
/// The derived UIDs, once created, are independent and do not require sequencing on the parent
/// object. They can be used without affecting the parent. The parent only maintains a record of
/// which derived addresses have been claimed to prevent duplicates.
module sui::derived_object;

use sui::dynamic_field as df;

/// Tries to create an object twice with the same parent-key combination.
const EObjectAlreadyExists: u64 = 0;
/// Tries to restore an object that does not exist for the supplied parent.
const EInvalidParent: u64 = 1;
/// Tries to use functionality that is not supported yet.
const ENotSupported: u64 = 2;

/// Added as a DF to the parent's UID, to mark an ID as claimed.
public struct Claimed(ID) has copy, drop, store;

/// An internal key to protect from generating the same UID twice (e.g. collide with DFs)
public struct DerivedObjectKey<K: copy + drop + store>(K) has copy, drop, store;

/// Claim a deterministic UID, using the parent's UID & any key.
public fun claim<K: copy + drop + store>(parent: &mut UID, key: K): UID {
    let addr = derive_address(parent.to_inner(), key);
    let id = addr.to_id();

    // If the UID has never been claimed, we can generate it and return early.
    if (!df::exists_(parent, Claimed(id))) {
        let uid = object::new_uid_from_hash(addr);

        // We save the value as `Option<UID>` to allow us to have "restore" functionality for
        // a derived UID.
        df::add<_, Option<UID>>(parent, Claimed(id), option::none());

        return uid
    };

    // IF the UID has been restored, we can re-use it.
    let existing_uid = df::borrow_mut<_, Option<UID>>(parent, Claimed(id));

    assert!(existing_uid.is_some(), EObjectAlreadyExists);

    abort ENotSupported
    // TODO: Enable once id leak verifier has been removed.
    // existing_uid.extract()
}

/// Return a `UID`, making it reclaimable in the future.
/// Note: This is not yet supported.
/// TODO: Should we make this public(package) or internal until we are indeed able to support
/// reclaims?
public fun restore(parent: &mut UID, uid: UID) {
    let id = uid.to_inner();
    assert!(df::exists_(parent, Claimed(id)), EInvalidParent);

    let claimed: &mut Option<UID> = df::borrow_mut(parent, Claimed(id));
    claimed.fill(uid);

    abort ENotSupported
}

/// Checks if a provided `key` has been claimed in the past. This does not guarantee
/// that the UID is still live (it might have been deleted.)
public fun exists<K: copy + drop + store>(parent: &UID, key: K): bool {
    let addr = derive_address(parent.to_inner(), key);
    df::exists_(parent, Claimed(addr.to_id()))
}

/// Given an ID and a Key, it calculates the derived address.
public fun derive_address<K: copy + drop + store>(parent: ID, key: K): address {
    df::hash_type_and_key(parent.to_address(), DerivedObjectKey(key))
}
