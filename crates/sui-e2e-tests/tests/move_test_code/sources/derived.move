// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(lint(self_transfer))]
module move_test_code::derived;

use sui::derived_object;
use sui::dynamic_field;
use sui::transfer::Receiving;

public struct Parent has key, store {
  id: UID,
}

public struct Derived has key, store {
  id: UID,
}

public struct AnyObj has key, store {
  id: UID,
}

public fun create_parent(ctx: &mut TxContext) {
  let parent = Parent { id: object::new(ctx) };

  transfer::public_transfer(parent, ctx.sender());
}

public fun create_any_obj(recipient: address, ctx: &mut TxContext) {
  let any_obj = AnyObj { id: object::new(ctx) };

  transfer::public_transfer(any_obj, recipient);
}

public fun create_derived(parent: &mut Parent, key: u64, ctx: &TxContext) {
  let derived = Derived { id: derived_object::claim(&mut parent.id, key) };

  transfer::public_transfer(derived, ctx.sender());
}

public fun create_derived_with_df(parent: &mut Parent, key: u64, ctx: &TxContext) {
  let mut derived_uid = derived_object::claim(&mut parent.id, key);

  dynamic_field::add(&mut derived_uid, b"key", b"value");

  let mut derived = Derived { id: derived_uid };

  transfer::public_transfer(derived, ctx.sender());
}

public fun receive(derived: &mut Derived, receiving: Receiving<AnyObj>, ctx: &mut TxContext) {
  let item = transfer::receive(&mut derived.id, receiving);
  transfer::public_transfer(item, ctx.sender());
}

public fun claim_and_receive(
  parent: &mut Parent,
  key: u64,
  receiver: Receiving<AnyObj>,
  ctx: &mut TxContext,
) {
  let mut derived_uid = derived_object::claim(&mut parent.id, key);

  let claimed = transfer::public_receive(&mut derived_uid, receiver);

  transfer::public_transfer(Derived { id: derived_uid }, ctx.sender());
  transfer::public_transfer(claimed, ctx.sender());
}
