// WP-115 item 5, H1 (digest §9): a block-scope alias the printed signature of a local function mentions (as written) is part of its pretty name's identity (clang-19: ...L::m(Al) against ...L::m(Bl)): G changes (f() itself does not: a local type's name is no content of the function that declares it)
consteval const char *f() { using Al = int; struct L { static consteval const char *m(Al) { return __PRETTY_FUNCTION__; } }; return L::m(0); }
constexpr const char *G = f();
