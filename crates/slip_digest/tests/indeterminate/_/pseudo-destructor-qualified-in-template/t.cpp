#include "side.h"
// WP-035's pair of `CXXPseudoDestructorExpr.aux-qualified`, moved by WP-041 fix round 3 (§4.4 "Spellings", rule 11): the bit is written inside an instantiation-dependent expression only, so always in a templated stream, where Kiln keeps that the name was qualified and, for a qualifier that is not dependent itself, not the qualifier (which can hold an instantiation-dependent operand). Side A, qualified, is indeterminate (`spelling/pseudo-destructor-qualifier`); no pair can differ on the bit alone, so the row is in `tests/tables/untested.txt`.
typedef int I;
I *p;
template <class T> struct A {
#ifdef B
  static inline int v = (p->~I(), sizeof(T));
#else
  static inline int v = (p->I::~I(), sizeof(T));
#endif
};
