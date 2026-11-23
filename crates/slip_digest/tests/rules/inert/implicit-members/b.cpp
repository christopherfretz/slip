namespace a { namespace impl { int k = 3; } int use() { return impl::k; } }
template <class T> struct W { T v; };
int f(W<int> a) { W<int> b = a; b = a; return b.v; }
