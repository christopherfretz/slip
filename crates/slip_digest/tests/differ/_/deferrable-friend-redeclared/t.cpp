#include "side.h"
// digest-kinds §11.2 rule 3: g's first declaration is the friend A<char>
// instantiates, whose pattern's `noexcept` is value-dependent; the
// namespace-scope redeclaration is g's representative. It is written in
// source, not made by instantiation (`templ::made_by_instantiation`, the
// test `demand::instantiated` reads since WP-045), and Kiln resolves the
// friend's specification at it: g is not of the deferrable class, and the
// specification is in g's own unit, which differs (WP-046 judged the
// redeclaration through its entity, an over-inclusion that put it in the
// resolved unit). `changed A::friend g(int)` is the friend in A's
// pattern, whose operand names N (written since WP-036).
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
