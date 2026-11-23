namespace a { namespace impl { int k = 3; } int use() { return impl::k; } }
template <class T> struct W { T v; template <class U> operator U() const { return U(v); } };
int f(W<bool> s) { return int(s); }
