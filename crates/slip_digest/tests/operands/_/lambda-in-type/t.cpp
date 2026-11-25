#include "side.h"
// digest §4.3, §2.4 (WP-156): a closure whose lambda-expression only a
// type holds (a template argument). Before WP-156 its owner's numbering
// did not find it (x's stream holds no initialiser) and what named it
// was indeterminate (`UndeclaredLocal`); the operand unit of x walks the
// `decltype` operand in x's numbering, which now numbers the closure, so
// every unit is determinate and equal.
template <class T> struct X {};
X<decltype([] {})> x;
