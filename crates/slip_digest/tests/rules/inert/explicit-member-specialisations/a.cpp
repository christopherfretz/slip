namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
template <class T> struct W { T v; int f() const { return 1; } operator T() const { return v; } struct In { int a; operator int() const { return a; } }; template <class U> struct I2 { U u; }; };
template <> int W<int>::f() const { return 2; }
template <> W<long>::operator long() const { return 3; }
template <> struct W<int>::In { long b; operator long() const { return b; } };
template <> template <class U> struct W<char>::I2 { U uu; operator U() const { return uu; } };
int f(W<int> a, W<long> b) { W<int>::In i{1}; W<char>::I2<int> j{2}; W<long>::In k{3}; return a.f() + int(b) + int(i) + j + k; }
