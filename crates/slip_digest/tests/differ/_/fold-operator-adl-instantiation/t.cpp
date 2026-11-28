#include "side.h"
// WP-138 item 2: an operator overload declared after a fold template in an
// operand's namespace is found by ADL at the instantiation for that operand
// type (moving that instantiation), while the template pattern and an
// instantiation for another type are unchanged.
namespace A {
struct X {
  int v;
  operator int() const { return v; }
};
}
namespace C {
struct Y { int v; };
Y operator+(Y a, Y b) { return {a.v + b.v}; }
}
template<class... T> auto f(T... t) { return (t + ...); }
namespace A {
#ifdef B
int operator+(const X &a, const X &b) { return a.v + b.v + 1; }
#endif
}
int u = f(A::X{1}, A::X{2});
C::Y v = f(C::Y{1}, C::Y{2});
