// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

/// Converts an arbitrary instance ID into a filesystem-safe storage key.
///
/// Each byte of the ID is hex-encoded as two lowercase digits and prefixed with
/// `id-`. Hex encoding is used (rather than naive character substitution) because
/// it is injective: it prevents path traversal (e.g. `../tenant` →
/// `id-2e2e2f74656e616e74`), eliminates separator collisions (e.g. `a/b` and
/// `a_b` map to distinct keys), and always yields a valid single-segment
/// directory name on every platform. Used to derive the per-instance index and
/// WAL directories under `./data/`.
pub(crate) fn instance_storage_key(instance_id: &str) -> String {
    let mut key = String::with_capacity(3 + instance_id.len() * 2);
    key.push_str("id-");

    for byte in instance_id.bytes() {
        let high = byte >> 4;
        let low = byte & 0x0f;
        key.push(char::from(b"0123456789abcdef"[usize::from(high)]));
        key.push(char::from(b"0123456789abcdef"[usize::from(low)]));
    }

    key
}

#[cfg(test)]
mod tests {
    use super::instance_storage_key;

    #[test]
    fn instance_storage_key_is_collision_resistant_for_separator_variants() {
        assert_ne!(instance_storage_key("a/b"), instance_storage_key("a_b"));
        assert_ne!(instance_storage_key("a\\b"), instance_storage_key("a_b"));
    }

    #[test]
    fn instance_storage_key_encodes_empty_id() {
        assert_eq!(instance_storage_key(""), "id-");
    }

    #[test]
    fn instance_storage_key_encodes_ascii_id() {
        assert_eq!(instance_storage_key("default"), "id-64656661756c74");
    }

    #[test]
    fn instance_storage_key_encodes_path_traversal_characters() {
        let key = instance_storage_key("../tenant");
        assert_eq!(key, "id-2e2e2f74656e616e74");
        assert!(!key.contains('/'));
        assert!(!key.contains('\\'));
    }
}
