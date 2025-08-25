// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses a=0x0 --accounts A

//# publish
module a::m;

use sui::derived_object;

public struct Obj has key {
  id: object::UID,
}

public struct DerivedObj has key {
  id: object::UID,
}

entry fun t1(ctx: &mut TxContext) {
  sui::transfer::transfer(Obj { id: object::new(ctx) }, ctx.sender());
}

entry fun t2(obj: &mut Obj, key: u64, ctx: &TxContext) {
  let id = derived_object::claim(&mut obj.id, key);
  sui::transfer::transfer(DerivedObj { id }, ctx.sender());
}

//# run a::m::t1 --sender A

//# run a::m::t2 --sender A --args object(2,0) 0

//# run a::m::t2 --sender A --args object(2,0) 1



