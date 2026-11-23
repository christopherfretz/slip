namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
template <class T> struct W { T v; operator T() const { return v; } };
int f(W<bool> s) { if (s) return 1; return 0; }
