#include "side.h"
// digest-kinds §11.2 rule 3: g's first declaration is the friend A<char>
// instantiates, whose pattern's `noexcept` is value-dependent; the
// namespace-scope redeclaration is g's representative, and it is judged
// made by instantiation through its entity (`templ::instantiated_declaration`),
// so g is of the deferrable class (an over-inclusion: Kiln resolves the
// friend's specification at the redeclaration). The specification is in
// the resolved unit on both sides, and differs there; g's own unit is
// equal. `changed A::friend g(int)` is the friend in A's pattern, whose operand
// names N (written since WP-036).
#ifdef B
#define N 0
#else
#define N 1
#endif
template <class T> struct A { friend void g(int) noexcept(sizeof(T) > N); };
A<char> a;
#ifdef B
void g(int) noexcept(true);
#else
void g(int) noexcept(false);
#endif
