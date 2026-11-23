// std: c++26
namespace a { namespace impl { int k = 3; } int use() { return impl::k; } }
template <class T> struct W { struct { T v; operator T() const { return v; } } _; };
int f(W<bool> w) { if (w._) return 1; return 0; }
