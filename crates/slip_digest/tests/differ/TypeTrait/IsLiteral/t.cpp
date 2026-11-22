#include "side.h"
// §7 TypeTrait: `IsLiteral`: against `__is_lvalue_reference`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_lvalue_reference(T); };
#else
template <class T> struct A { unsigned long w = __is_literal(T); };
#endif
