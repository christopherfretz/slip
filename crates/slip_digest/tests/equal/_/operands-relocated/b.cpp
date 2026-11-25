// WP-156: the mirror of the operand pairs: a correct relocation of the
// namespace an operand names leaves every unit, operand units included,
// equal (the references are keys, mapped).
namespace acme { struct X {}; int make(X); template <class T> using id = T; }
acme::X x;
decltype(make(x)) v;
__typeof__(acme::make(x)) t;
template <class T, unsigned long N> struct arr {};
arr<char, sizeof(acme::make(x))> a;
void f() noexcept(noexcept(acme::make(x)));
acme::id<int> i;
template <class T> struct W { decltype(make(T())) m; arr<char, sizeof(make(T()))> n; };
W<acme::X> w;
template <class T> auto r(T t) -> decltype(make(t));
int u = r(x);
