namespace a { namespace detail { struct X { int k = 3; }; } int use() { return detail::X().k; } }
template <class T> struct W { T v; operator T() const { return v; } };
template <class T> struct V { T v; operator a::detail::X() const { return {}; } };
int f(W<a::detail::X> s, V<int> u) { a::detail::X x = s; a::detail::X z = u; return x.k + z.k; }
