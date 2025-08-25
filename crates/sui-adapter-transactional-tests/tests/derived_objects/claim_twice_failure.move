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
  let mut id = object::new(ctx);

  let id2 = derived_object::claim(&mut id, 0u64);

  sui::transfer::transfer(Obj { id }, ctx.sender());
  sui::transfer::transfer(DerivedObj { id: id2 }, ctx.sender());
}

entry fun t2(obj: &mut Obj, key: u64) {
  let id3 = derived_object::claim(&mut obj.id, key);
  id3.delete();
}

//# run a::m::t1 --sender A

//# view-object 2,0

//# view-object 2,1

//# view-object 2,2

//# run a::m::t2 --sender A --args object(2,1) 0


