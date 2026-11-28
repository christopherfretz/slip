#include "side.h"
// WP-138 item 2: an operator overload declared before a fold template moves
// the template pattern (its `oper_lookups` set changed) and the instantiation
// whose operator resolution switches to it, while an unrelated instantiation
// stays equal.
namespace A {
struct X {
  int v;
  operator int() const { return v; }
};
}
#ifdef B
int operator+(const A::X &a, const A::X &b) { return a.v + b.v + 1; }
#endif
template<class... T> auto f(T... t) { return (t + ...); }
int u = f(A::X{1}, A::X{2});
int w = f(1, 2);
