namespace a { namespace impl { int k = 3; } int use() { return impl::k; } }
template <class T> struct W { enum {}; enum {}; T v; };
int f(W<int> w) { return w.v; }
