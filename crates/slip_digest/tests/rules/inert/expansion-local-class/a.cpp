// std: c++26
namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
int h() { int s = 0; template for (constexpr int i : {1, 2}) { struct L { int m; int get() const { return m + i; } }; L l{i}; s += l.get(); } return s; }
template <class T> T g() { T s = 0; template for (constexpr T i : {T(1), T(2)}) { struct M { T m; }; s += M{i}.m; } return s; }
int f() { return g<int>(); }
