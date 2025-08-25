use crate::{
    base_types::{ObjectID, SuiAddress},
    crypto::DefaultHash,
    SUI_FRAMEWORK_ADDRESS,
};
use fastcrypto::hash::HashFunction;
use move_core_types::{
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
};
use shared_crypto::intent::HashingIntentScope;

/// Taking in a parent T and a key (Type & bytes),
/// we derive the object id the same way `derived_object.move` does on
/// sui framework.
pub fn derive_object_id(
    parent_id: ObjectID,
    key_type_tag: &TypeTag,
    key_bytes: &[u8],
) -> Result<ObjectID, bcs::Error> {
    let parent: SuiAddress = parent_id.into();

    // Wrap `T` into `DerivedObjectKey<T>` type (preserving namespacing).
    let wrapper_type_tag = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: Identifier::new("derived_object").unwrap(),
        name: Identifier::new("DerivedObjectKey").unwrap(),
        type_params: vec![key_type_tag.clone()],
    }));

    let k_tag_bytes = bcs::to_bytes(&wrapper_type_tag)?;

    tracing::trace!(
        "Deriving object ID (derived_object) for parent={:?}, key={:?}, key_type_tag={:?}",
        parent,
        key_bytes,
        wrapper_type_tag,
    );

    // hash(parent || len(key) || key || key_type_tag)
    let mut hasher = DefaultHash::default();
    hasher.update([HashingIntentScope::ChildObjectId as u8]);
    hasher.update(parent);
    hasher.update(key_bytes.len().to_le_bytes());
    hasher.update(key_bytes);
    hasher.update(k_tag_bytes);
    let hash = hasher.finalize();

    // truncate into an ObjectID and return
    // OK to access slice because digest should never be shorter than ObjectID::LENGTH.
    let id = ObjectID::try_from(&hash.as_ref()[0..ObjectID::LENGTH]).unwrap();
    tracing::trace!("derive_object_id result: {:?}", id);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use serde::Serialize;

    use super::*;

    #[derive(Serialize)]
    struct DemoStruct {
        value: u64,
    }

    #[derive(Serialize)]
    struct GenericStruct<T> {
        value: T,
    }

    // Snapshot tests that match the on-chain `derive_address` logic.
    // Similar tests can be found in `derived_object_tests.move`
    #[test]
    fn test_derive_object_snapshot() {
        // Our key is `UID, Vec<u8>, b"foo"`
        let key_bytes = bcs::to_bytes("foo".as_bytes()).unwrap();
        let key_type_tag = TypeTag::Vector(Box::new(TypeTag::U8));

        let id = derive_object_id(
            ObjectID::from_str("0x2").unwrap(),
            &key_type_tag,
            &key_bytes,
        )
        .unwrap();

        assert_eq!(
            id,
            ObjectID::from_str(
                "0xa2b411aa9588c398d8e3bc97dddbdd430b5ded7f81545d05e33916c3ca0f30c3"
            )
            .unwrap()
        );
    }

    #[test]
    fn test_derive_object_with_struct_key_snapshot() {
        let key = DemoStruct { value: 1 };
        let key_value = bcs::to_bytes(&key).unwrap();

        let id = derive_object_id(
            ObjectID::from_str("0x2").unwrap(),
            &TypeTag::Struct(Box::new(StructTag {
                address: SUI_FRAMEWORK_ADDRESS,
                module: Identifier::new("derived_object_tests").unwrap(),
                name: Identifier::new("DemoStruct").unwrap(),
                type_params: vec![],
            })),
            &key_value,
        )
        .unwrap();

        assert_eq!(
            id,
            ObjectID::from_str(
                "0x20c58d8790a5d2214c159c23f18a5fdc347211e511186353e785ad543abcea6b"
            )
            .unwrap()
        );
    }

    #[test]
    fn test_derive_object_with_generic_struct_key_snapshot() {
        let key = GenericStruct::<u64> { value: 1 };
        let key_value = bcs::to_bytes(&key).unwrap();

        let id = derive_object_id(
            ObjectID::from_str("0x2").unwrap(),
            &TypeTag::Struct(Box::new(StructTag {
                address: SUI_FRAMEWORK_ADDRESS,
                module: Identifier::new("derived_object_tests").unwrap(),
                name: Identifier::new("GenericStruct").unwrap(),
                type_params: vec![TypeTag::U64],
            })),
            &key_value,
        )
        .unwrap();

        assert_eq!(
            id,
            ObjectID::from_str(
                "0xb497b8dcf1e297ae5fa69c040e4a08ef8240d5373bbc9d6b686ffbd7dfe04cbe"
            )
            .unwrap()
        );
    }
}
