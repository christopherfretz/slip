namespace a { namespace impl { int k = 3; } int use() { return impl::k; } }
template <class T> int g(T t) { auto l = [](T x) { return int(x); }; int (*p)(T) = l; return p(t); }
int f() { return g(1); }
