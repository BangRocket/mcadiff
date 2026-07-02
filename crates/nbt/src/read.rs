//! NBT binary reader — delegates parsing of (untrusted) Java NBT bytes to the
//! battle-tested `valence_nbt`, then converts into our uniform [`NbtValue`].

use crate::conv::from_compound;
use crate::value::NbtValue;
use crate::{NbtError, Result};

/// Read a complete NBT document: returns the root tag's name and value. The NBT
/// root is always a compound.
///
/// Parsing is recursion-depth-limited (valence_nbt rejects nesting past 512
/// levels), so hostile input cannot overflow the stack — every recursive walk
/// downstream (conversion, canonical bytes, comparer) inherits that bound.
pub fn read(buf: &[u8]) -> Result<(String, NbtValue)> {
    let mut slice = buf;
    let (compound, name) = valence_nbt::from_binary::<String>(&mut slice)
        .map_err(|e| NbtError::Binary(e.to_string()))?;
    Ok((name, NbtValue::Compound(from_compound(compound))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_named_compound_with_int() {
        // tag=10 (Compound), name="root", child tag=3 (Int) name="n" value=5, End
        let bytes = [
            10, // compound
            0, 4, b'r', b'o', b'o', b't', // name "root"
            3, 0, 1, b'n', 0, 0, 0, 5, // int "n" = 5
            0, // end
        ];
        let (name, val) = read(&bytes).unwrap();
        assert_eq!(name, "root");
        let NbtValue::Compound(m) = val else {
            panic!("expected compound")
        };
        assert_eq!(m.get("n"), Some(&NbtValue::Int(5)));
    }

    #[test]
    fn truncated_input_errors() {
        assert!(read(&[10, 0, 4, b'r']).is_err());
    }

    /// `depth` nested compounds under an empty-named root: root{a{a{…}}}.
    fn nested_compounds(depth: usize) -> Vec<u8> {
        let mut bytes = vec![10, 0, 0]; // compound, empty root name
        for _ in 0..depth {
            bytes.extend_from_slice(&[10, 0, 1, b'a']); // child compound "a"
        }
        bytes.resize(bytes.len() + depth + 1, 0); // End for every compound
        bytes
    }

    /// A hostile document nested past the decoder's recursion cap must be
    /// rejected with a depth error, not overflow the stack. Pins the
    /// valence_nbt MAX_DEPTH (512) guard so a parser swap or dependency bump
    /// cannot silently drop the protection.
    #[test]
    fn absurdly_nested_input_errors_instead_of_overflowing() {
        let err = read(&nested_compounds(600)).unwrap_err();
        assert!(
            err.to_string().contains("recursion"),
            "expected the depth guard to reject the input, got: {err}"
        );
    }

    /// Deep-but-legal nesting (far past any real chunk) parses, converts, and
    /// canonicalizes — the guard must not reject sane worlds.
    #[test]
    fn deep_but_sane_nesting_parses_and_canonicalizes() {
        let (_, v) = read(&nested_compounds(100)).unwrap();
        let mut cur = &v;
        let mut walked = 0;
        while let NbtValue::Compound(m) = cur {
            let Some(child) = m.get("a") else { break };
            cur = child;
            walked += 1;
        }
        assert_eq!(walked, 100);
        assert!(!crate::canonical_bytes(&v).is_empty());
    }
}
