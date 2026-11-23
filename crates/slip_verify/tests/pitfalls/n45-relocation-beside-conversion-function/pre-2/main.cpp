namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
template <class T> struct W { T v; operator T() const { return v; } template <class U> operator U() const { return U(v); } };
int f(W<int> s) { return s; }
long g(W<int> s) { return s; }
