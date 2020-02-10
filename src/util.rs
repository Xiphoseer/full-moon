use crate::tokenizer::TokenReference;
use std::{
    borrow::{Borrow, Cow},
    fmt::Display,
};

pub fn display_option<T: Display, O: Borrow<Option<T>>>(option: O) -> String {
    match option.borrow() {
        Some(x) => x.to_string(),
        None => "".to_string(),
    }
}

pub fn display_optional_punctuated<T: Display>(
    pair: &(T, Option<Cow<'_, TokenReference<'_>>>),
) -> String {
    format!("{}{}", pair.0, display_option(&pair.1))
}

pub fn display_optional_punctuated_vec<T: Display>(
    vec: &[(T, Option<Cow<'_, TokenReference<'_>>>)],
) -> String {
    let mut string = String::new();

    for pair in vec {
        string.push_str(&display_optional_punctuated(pair));
    }

    string
}

pub fn join_vec<T: Display, V: AsRef<[T]>>(vec: V) -> String {
    let mut string = String::new();

    for item in vec.as_ref() {
        string.push_str(&item.to_string());
    }

    string
}