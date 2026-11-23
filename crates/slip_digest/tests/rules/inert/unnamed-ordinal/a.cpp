namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
template <class T> struct W { enum {}; enum {}; T v; };
int f(W<int> w) { return w.v; }
