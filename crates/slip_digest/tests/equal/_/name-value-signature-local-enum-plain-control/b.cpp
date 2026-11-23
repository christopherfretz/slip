// WP-115 item 5, H1 control (digest §9): __func__ prints no signature, so a local type renamed in it leaves G equal
consteval const char *f() { enum F { X }; struct L { static consteval const char *m(F) { return __func__; } }; return L::m(X); }
constexpr const char *G = f();
