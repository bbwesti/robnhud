//! CRYPTEK VIGIL — Core types, config, and database.

pub fn hello() -> &'static str {
    "Vigil Core online"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello() {
        assert_eq!(hello(), "Vigil Core online");
    }
}
