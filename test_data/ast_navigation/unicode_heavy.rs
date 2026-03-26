/// A struct with Unicode identifiers and strings.
struct Données {
    /// Multi-byte field name
    résultat: String,
    /// Japanese field
    名前: String,
}

impl Données {
    fn new() -> Self {
        Self {
            résultat: "réussi — très bien 🎉".to_string(),
            名前: "テスト".to_string(),
        }
    }

    fn afficher(&self) -> String {
        format!("{}: {}", self.名前, self.résultat)
    }
}

fn main() {
    let données = Données::new();
    println!("{}", données.afficher());
}
