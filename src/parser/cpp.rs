pub(crate) const LANG_NAME: &str = "c++";
pub(crate) const LANG_EXTENSIONS: &[&str] = &[
    "cc", "cpp", "CPP", "cxx", "c++", "cp", "C", "cppm", "ixx", "ii", "H", "hh", "hpp", "HPP",
    "hxx", "h++", "tcc",
];

/// C uses the pinned C++ grammar, but has independent generated tag and kind tables.
pub(crate) const C_LANG_NAME: &str = "c";
pub(crate) const C_LANG_EXTENSIONS: &[&str] = &["c", "h", "i"];
