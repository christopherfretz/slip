namespace a { namespace impl { int k = 3; } int use() { return impl::k; } }
template <class T> struct W { T v; operator long() const { return v; } };
int f(W<bool> s) { if (s) return 1; return 0; }
