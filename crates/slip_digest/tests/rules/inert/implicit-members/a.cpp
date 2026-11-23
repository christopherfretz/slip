namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
template <class T> struct W { T v; };
int f(W<int> a) { W<int> b = a; b = a; return b.v; }
