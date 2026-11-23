namespace a { namespace impl { int k = 3; } int use() { return impl::k; } }
template <class T> struct W { struct { T a; } s; union { T b; int c; }; enum { E = sizeof(T) }; };
int f(W<int> w) { return w.s.a + w.b + W<int>::E; }
