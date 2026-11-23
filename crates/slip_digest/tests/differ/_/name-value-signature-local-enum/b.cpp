// WP-115 item 5, H1 (digest §9): a named local type the printed signature of a local function mentions is part of its pretty name's identity (clang-19: ...L::m(F) against ...L::m(F)): G changes (f() itself does not: a local type's name is no content of the function that declares it)
consteval const char *f() { enum F { X }; struct L { static consteval const char *m(F) { return __PRETTY_FUNCTION__; } }; return L::m(X); }
constexpr const char *G = f();
