#include "side.h"
// digest-kinds §11.2 rule 3: g's first declaration is the friend A<char>
// instantiates, whose pattern's `noexcept` is value-dependent; the
// namespace-scope redeclaration is g's representative, and it is judged
// made by instantiation through its entity (`templ::instantiated_declaration`),
// so g is of the deferrable class (an over-inclusion: Kiln resolves the
// friend's specification at the redeclaration). The specification is in
// the resolved unit on both sides, and differs there only.
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
