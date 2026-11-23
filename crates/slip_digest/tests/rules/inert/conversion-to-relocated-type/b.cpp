namespace a { namespace impl { struct X { int k = 3; }; } int use() { return impl::X().k; } }
template <class T> struct W { T v; operator T() const { return v; } };
template <class T> struct V { T v; operator a::impl::X() const { return {}; } };
int f(W<a::impl::X> s, V<int> u) { a::impl::X x = s; a::impl::X z = u; return x.k + z.k; }
