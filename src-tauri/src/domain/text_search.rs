//! Filtre Texte de la grille : les termes saisis, la règle qui
//! les compare au texte d'une photo, sans tenir compte des majuscules ni des
//! accents(« ete » trouve « Été »).

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Champ cherché.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextField {
    /// Nom de fichier, titre, légende, auteur, copyright, lieu et
    /// mots-clés (avec leurs parents).
    All,
    /// Nom de fichier.
    FileName,
    /// Titre.
    Title,
    /// Légende.
    Caption,
    /// Mots-clés posés et leurs parents (« France » trouve une photo de
    /// « Lieux > France > Paris »).
    Keywords,
    /// Pays, région, ville et lieu précis.
    Place,
}

/// Règle de comparaison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextRule {
    /// Au moins un terme apparaît, même dans un mot.
    Contains,
    /// Tous les termes apparaissent, même dans un mot.
    ContainsAll,
    /// Tous les termes apparaissent comme mots entiers.
    ContainsWords,
    /// Aucun terme n'apparaît.
    DoesNotContain,
    /// Un mot commence par l'un des termes.
    StartsWith,
    /// Un mot se termine par l'un des termes.
    EndsWith,
}

/// Texte sans majuscules ni accents, pour comparer (« Été » → « ete »).
#[must_use]
pub fn search_key(text: &str) -> String {
    text.nfd()
        .filter(|c| !is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
}

/// Termes saisis, sans majuscules ni accents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextTerms {
    /// Termes que la règle compare.
    include: Vec<String>,
    /// Termes précédés de « ! » : une photo qui en contient un est écartée,
    /// quelle que soit la règle.
    exclude: Vec<String>,
}

impl TextTerms {
    /// Découpe la saisie : termes séparés par des virgules ou des espaces,
    /// « ! » devant un terme l'exclut (`portrait, !enfants`).
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut terms = Self::default();
        for term in text.split(|c: char| c == ',' || c.is_whitespace()) {
            let (list, term) = match term.strip_prefix('!') {
                Some(excluded) => (&mut terms.exclude, excluded),
                None => (&mut terms.include, term),
            };
            if !term.is_empty() {
                list.push(search_key(term));
            }
        }
        terms
    }

    /// Vrai s'il n'y a aucun terme : le filtre laisse tout passer.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }

    /// Indique si le texte d'une photo (`text`, tous ses champs) est retenu
    /// par la règle.
    #[must_use]
    pub fn matches(&self, rule: TextRule, text: &str) -> bool {
        self.matches_key(rule, &search_key(text))
    }

    /// Comme [`Self::matches`], pour un texte déjà passé par
    /// [`search_key`] : un texte commun à beaucoup de photos (le chemin
    /// d'un mot-clé) n'est ainsi préparé qu'une fois.
    #[must_use]
    pub fn matches_key(&self, rule: TextRule, text: &str) -> bool {
        if self.exclude.iter().any(|term| text.contains(term.as_str())) {
            return false;
        }
        if self.include.is_empty() {
            return true;
        }
        let words = || text.split(|c: char| !c.is_alphanumeric());
        let mut include = self.include.iter();
        match rule {
            TextRule::Contains => include.any(|term| text.contains(term.as_str())),
            TextRule::ContainsAll => include.all(|term| text.contains(term.as_str())),
            TextRule::ContainsWords => include.all(|term| words().any(|word| word == term)),
            TextRule::DoesNotContain => !include.any(|term| text.contains(term.as_str())),
            TextRule::StartsWith => {
                include.any(|term| words().any(|word| word.starts_with(term.as_str())))
            }
            TextRule::EndsWith => {
                include.any(|term| words().any(|word| word.ends_with(term.as_str())))
            }
        }
    }
}
