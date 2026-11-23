// WP-104 (digest §9), M2h: two instantiations share the pattern's extent; only the name check tells g<int>'s string from g<long>'s
template <class T> constexpr const char *g(T) { return __PRETTY_FUNCTION__; }
consteval const char *pick(bool b) { return b ? g(1) : g(1L); }
constexpr const char *P = pick(true);
