#include "side.h"
// digest-kinds §11.2 rule 3: an implicit instantiation whose pattern has
// a value-dependent `noexcept` operand. A call, even in an unevaluated
// operand, resolves it: the unit is on both sides and a second use
// changes nothing (`function-template` shows it appear).
template <class T> struct A { void f() noexcept(sizeof(T) > 1); };
void k(A<int> &a) { (void)sizeof((a.f(), 0)); }
#ifdef B
bool h(A<int> &a) { return noexcept(a.f()); }
#endif
