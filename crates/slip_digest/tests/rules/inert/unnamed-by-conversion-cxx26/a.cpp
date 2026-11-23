// std: c++26
namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
template <class T> struct W { struct { operator T() const { return v; } T v; } _; };
int f(W<bool> w) { if (w._) return 1; return 0; }
