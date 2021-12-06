use crate::traits::Hash;

impl Hash for String {
    fn hash(&self) -> String {
        blake3::hash(self.as_bytes()).to_string()
    }
}
