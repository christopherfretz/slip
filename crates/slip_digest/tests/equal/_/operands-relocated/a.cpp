// WP-156: the mirror of the operand pairs: a correct relocation of the
// namespace an operand names leaves every unit, operand units included,
// equal (the references are keys, mapped).
namespace legacy { struct X {}; int make(X); template <class T> using id = T; }
legacy::X x;
decltype(make(x)) v;
__typeof__(legacy::make(x)) t;
template <class T, unsigned long N> struct arr {};
arr<char, sizeof(legacy::make(x))> a;
void f() noexcept(noexcept(legacy::make(x)));
legacy::id<int> i;
template <class T> struct W { decltype(make(T())) m; arr<char, sizeof(make(T()))> n; };
W<legacy::X> w;
template <class T> auto r(T t) -> decltype(make(t));
int u = r(x);
